use crate::impl_tui_table_state;
use crate::tui::common::base_table_state::BaseTableState;
use crate::tui::common::key_handler::{
    KeyHandlerResult, handle_common_keys, handle_filter_editing_keys,
};
use crate::tui::data::{ResourceEvent, event_constraint_len_calculator};
use crate::tui::event_app;
use crate::tui::stream::Message;
use crate::tui::style::ITEM_HEIGHT;
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{AppBehavior, Apps};
use crate::{
    cache_manager,
    k8s::cache::{DataRequest, FetchResult, ResourceRef},
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use futures::Stream;
use ratatui::prelude::*;
use ratatui::widgets::ScrollbarState;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_stream::wrappers::ReceiverStream;
use tracing::debug;

const POLL_MS: u64 = 1000;

#[derive(Clone, Debug)]
pub struct App {
    pub(crate) base: BaseTableState<ResourceEvent>,
    pub(crate) longest_item_lens: (u16, u16, u16, u16, u16),
}

impl_tui_table_state!(App, ResourceEvent);

impl AppBehavior for event_app::app::App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        if self.get_show_filter_edit() {
            return Ok(Some(self.handle_filter_edit_event(event)));
        }

        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    // First try common keys (navigation, quit, color, vim motions)
                    return match handle_common_keys(self, key, |app| Apps::Event { app }) {
                        KeyHandlerResult::Quit => Ok(None),
                        KeyHandlerResult::HandledWithUpdate(app_holder)
                        | KeyHandlerResult::Handled(app_holder) => Ok(app_holder),
                        KeyHandlerResult::NotHandled => {
                            // Handle Event-specific keys
                            Ok(Some(self.handle_event_specific_keys(key)))
                        }
                    };
                }
                Ok(Some(Apps::Event { app: self.clone() }))
            }
            Message::Event(data_vec) => Ok(Some(self.handle_data_update(data_vec))),
            _ => Ok(Some(Apps::Event { app: self.clone() })),
        }
    }

    fn draw_ui<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal.draw(|f| event_app::ui::ui(f, &mut self.clone()))?;
        Ok(())
    }

    fn stream(&self, should_stop: Arc<AtomicBool>) -> impl Stream<Item = Message> {
        let (tx, rx) = mpsc::channel(100);

        let initial_items = self.get_items().to_vec();

        tokio::spawn(async move {
            let cache = cache_manager::get_cache_or_default();
            let request = DataRequest::Events {
                resource: ResourceRef::All,
                limit: 100, // Reasonable limit for all events
            };

            // Subscribe to cache updates
            let (sub_id, mut cache_rx) = cache
                .subscription_manager
                .subscribe("events:*".to_string())
                .await;

            // Start with cached data if available
            if let Some(FetchResult::Events(cached_items)) = cache.get(&request).await {
                if !cached_items.is_empty()
                    && cached_items != initial_items
                    && tx.send(Message::Event(cached_items)).await.is_err()
                {
                    cache.subscription_manager.unsubscribe(&sub_id).await;
                    return;
                }
            }

            // Listen for cache updates or fallback to direct polling
            while !should_stop.load(Ordering::Relaxed) {
                tokio::select! {
                 // Try to get updates from cache first
                 update = cache_rx.recv() => {
                     if let Some(crate::k8s::cache::DataUpdate::Events(new_items)) = update {
                         if !new_items.is_empty() && new_items != initial_items && tx.send(Message::Event(new_items)).await.is_err() {
                             break;
                         }
                     }
                 }
                 // Fallback: check cache periodically and refresh if needed
                 () = sleep(Duration::from_millis(POLL_MS)) => {
                     // Try cache first
                     debug!("updating event app data...");
                     match cache.get(&request).await {
                         Some(FetchResult::Events(cached_items)) => {
                             debug!("⚡ Using cached Events data ({} items)", cached_items.len());
                             if !cached_items.is_empty() && cached_items != initial_items && tx.send(Message::Event(cached_items)).await.is_err() {
                                 break;
                             }
                         }
                         Some(_) => {
                             // Wrong data type in cache - should not happen
                             debug!("⚠️ Unexpected data type in cache for Event request");
                         }
                         None => {
                             // Cache miss - try stale data while background fetcher works
                             if let Some(FetchResult::Events(stale_items)) = cache.get_or_mark_stale(&request).await {
                                 if !stale_items.is_empty() && stale_items != initial_items && tx.send(Message::Event(stale_items)).await.is_err() {
                                     break;
                                 }
                             }
                         }
                     }
                 }
                }
            }

            cache.subscription_manager.unsubscribe(&sub_id).await;
        });

        ReceiverStream::new(rx)
    }
}

impl App {
    pub fn new() -> Self {
        let data_vec = vec![];
        Self {
            base: BaseTableState::new(data_vec.clone()),
            longest_item_lens: event_constraint_len_calculator(&data_vec),
        }
    }

    /// Handle Event-specific key events that aren't covered by common key handler
    fn handle_event_specific_keys(&mut self, key: &crossterm::event::KeyEvent) -> Apps {
        use KeyCode::{Char, Enter, Esc};

        match key.code {
            Esc => {
                // Navigate back to ReplicaSet page
                debug!("navigating back from event to rs...");
                Apps::Rs {
                    app: crate::tui::rs_app::app::App::new(vec![]),
                }
            }
            Char('/') => self.handle_filter_mode(),
            Enter => {
                // noop for now but will be pretty printed detail analysis popup
                Apps::Event { app: self.clone() }
            }
            _ => Apps::Event { app: self.clone() },
        }
    }

    /// Handle data update message
    fn handle_data_update(&self, data_vec: &[ResourceEvent]) -> Apps {
        debug!("updating event app data...");
        let mut new_app = self.clone();
        new_app.base.items = data_vec.to_vec();
        new_app.base.scroll_state =
            ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT);
        new_app.longest_item_lens = event_constraint_len_calculator(data_vec);
        Apps::Event { app: new_app }
    }

    /// Enter filter editing mode
    fn handle_filter_mode(&mut self) -> Apps {
        self.set_show_filter_edit(true);
        Apps::Event { app: self.clone() }
    }

    fn handle_filter_edit_event(&mut self, event: &Message) -> Apps {
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    if let Some(app) =
                        handle_filter_editing_keys(self, key, |app| Apps::Event { app })
                    {
                        return app;
                    }
                }
                Apps::Event { app: self.clone() }
            }
            Message::Event(data_vec) => self.handle_data_update(data_vec),
            _ => Apps::Event { app: self.clone() },
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
