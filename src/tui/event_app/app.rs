use crate::impl_tui_table_state;
use crate::{cache_manager, k8s::cache::{DataRequest, FetchResult, ResourceRef}};
use crate::tui::common::base_table_state::BaseTableState;
use crate::tui::data::{ResourceEvent, event_constraint_len_calculator};
use crate::tui::event_app;
use crate::tui::stream::Message;
use crate::tui::style::ITEM_HEIGHT;
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{AppBehavior, Apps};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
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
            Ok(self.handle_filter_edit_event(event))
        } else {
            Ok(self.handle_table_event(event))
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
                if !cached_items.is_empty() && cached_items != initial_items && tx.send(Message::Event(cached_items)).await.is_err() {
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

    fn handle_table_event(&mut self, event: &Message) -> Option<Apps> {
        let mut app_holder = Some(Apps::Event { app: self.clone() });
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Char, Down, Enter, Esc, Up};
                    match key.code {
                        Char('q') | Esc => {
                            app_holder = None;
                        }
                        Char('j') | Down => {
                            self.next();
                            //todo: stop all this cloning
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        Char('k') | Up => {
                            self.previous();
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        Char('c' | 'C') => {
                            self.next_color();
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        Char('f' | 'F') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_forward();
                        }
                        Char('b' | 'B') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_backward();
                        }
                        Enter => {
                            // noop for now but will be pretty printed detail analysis popup
                        }
                        Char('/') => {
                            self.set_show_filter_edit(true);
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        Char('G') => {
                            // Jump to bottom (vim motion)
                            self.jump_to_bottom();
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        Char('g') => {
                            // Jump to top (vim motion)
                            self.jump_to_top();
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        _k => {}
                    }
                }
            }
            Message::Event(data_vec) => {
                let mut new_app = self.clone();
                new_app.base.items.clone_from(data_vec);
                new_app.base.scroll_state =
                    ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT);
                new_app.longest_item_lens = event_constraint_len_calculator(data_vec);
                let new_app_holder = Apps::Event { app: new_app };
                app_holder = Some(new_app_holder);
            }
            _ => {}
        }
        app_holder
    }

    fn handle_filter_edit_event(&mut self, event: &Message) -> Option<Apps> {
        let mut app_holder = Some(Apps::Event { app: self.clone() });
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Backspace, Char, Enter, Esc, Left, Right};

                    match key.code {
                        Char(to_insert) => {
                            self.enter_char(to_insert);
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        Backspace => {
                            self.delete_char();
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        Left => {
                            self.move_cursor_left();
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        Right => {
                            self.move_cursor_right();
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        Esc | Enter => {
                            self.set_show_filter_edit(false);
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        _ => {}
                    }
                }
            }
            Message::Event(data_vec) => {
                debug!("updating event app data...");
                let mut new_app = self.clone();
                new_app.base.items.clone_from(data_vec);
                new_app.base.scroll_state =
                    ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT);
                new_app.longest_item_lens = event_constraint_len_calculator(data_vec);
                let new_app_holder = Apps::Event { app: new_app };
                app_holder = Some(new_app_holder);
            }
            _ => {}
        }
        app_holder
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
