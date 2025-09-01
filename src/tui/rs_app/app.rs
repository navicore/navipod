use crate::cache_manager;
use crate::k8s::cache::{DataRequest, FetchResult};
use crate::k8s::cache::config::DEFAULT_MAX_PREFETCH_REPLICASETS;
use crate::k8s::rs::list_replicas;
use crate::tui::data::Rs;
use crate::tui::pod_app;
// use crate::tui::rs_app::ui; // Unused while testing modern UI
use crate::tui::stream::Message;
use crate::tui::style::{TableColors, ITEM_HEIGHT, PALETTES};
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{create_ingress_data_vec, AppBehavior, Apps};
use crate::tui::{event_app, ingress_app};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use futures::Stream;
use ratatui::prelude::*;
use ratatui::widgets::{ScrollbarState, TableState};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, warn};

const POLL_MS: u64 = 5000;

/// Triggers prefetch of `Pod` data for the given `ReplicaSets`
///
/// This function generates prefetch requests for `Pod` data based on the selectors
/// from the provided `ReplicaSets`. It respects the configured limits for the number
/// of `ReplicaSets` to process to avoid overwhelming the system.
async fn trigger_replicaset_pod_prefetch(replicasets: &[Rs], context: &str) {
    use crate::k8s::cache::fetcher::PodSelector;
    
    if let Some(bg_fetcher) = cache_manager::get_background_fetcher() {
        let namespace = cache_manager::get_current_namespace_or_default();
        let mut prefetch_requests = Vec::new();

        // Generate Pod requests for each ReplicaSet
        for rs in replicasets.iter().take(DEFAULT_MAX_PREFETCH_REPLICASETS) {
            if let Some(selectors) = &rs.selectors {
                let pod_request = DataRequest::Pods {
                    namespace: namespace.clone(),
                    selector: PodSelector::ByLabels(selectors.clone()),
                };
                prefetch_requests.push(pod_request);
            }
        }

        if !prefetch_requests.is_empty() {
            debug!("🚀 {} PREFETCH: Scheduling {} Pod requests for {} ReplicaSets",
                   context, prefetch_requests.len(), replicasets.len());
            if let Err(e) = bg_fetcher.schedule_fetch_batch(prefetch_requests).await {
                warn!("Failed to schedule {} prefetch: {}", context, e);
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct App {
    pub(crate) state: TableState,
    pub(crate) items: Vec<Rs>,
    pub(crate) scroll_state: ScrollbarState,
    pub(crate) colors: TableColors,
    pub(crate) color_index: usize,
    pub(crate) filter: String,
    pub(crate) show_filter_edit: bool,
    pub(crate) edit_filter_cursor_position: usize,
}

impl TuiTableState for App {
    type Item = Rs;

    fn get_items(&self) -> &[Self::Item] {
        &self.items
    }

    fn get_state(&mut self) -> &mut TableState {
        &mut self.state
    }

    fn get_scroll_state(&self) -> &ScrollbarState {
        &self.scroll_state
    }

    fn set_scroll_state(&mut self, scroll_state: ScrollbarState) {
        self.scroll_state = scroll_state;
    }

    fn set_table_colors(&mut self, colors: TableColors) {
        self.colors = colors;
    }

    fn get_color_index(&self) -> usize {
        self.color_index
    }

    fn set_color_index(&mut self, color_index: usize) {
        self.color_index = color_index;
    }

    fn reset_selection_state(&mut self) {
        self.state = TableState::default().with_selected(0);
        self.scroll_state = ScrollbarState::new(self.items.len().saturating_sub(1) * ITEM_HEIGHT);
    }

    fn get_filter(&self) -> String {
        self.filter.clone()
    }

    fn set_filter(&mut self, filter: String) {
        self.filter = filter;
    }

    fn set_cursor_pos(&mut self, cursor_pos: usize) {
        self.edit_filter_cursor_position = cursor_pos;
    }

    #[allow(clippy::missing_const_for_fn)]
    fn get_cursor_pos(&self) -> usize {
        self.edit_filter_cursor_position
    }

    fn set_show_filter_edit(&mut self, show_filter_edit: bool) {
        self.show_filter_edit = show_filter_edit;
    }

    #[allow(clippy::missing_const_for_fn)]
    fn get_show_filter_edit(&self) -> bool {
        self.show_filter_edit
    }
}

impl AppBehavior for App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        if self.get_show_filter_edit() {
            Ok(self.handle_filter_edit_event(event))
        } else {
            self.handle_table_event(event).await
        }
    }

    fn draw_ui<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal.draw(|f| super::modern_ui::ui(f, self))?; // Use modern UI
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn stream(&self, should_stop: Arc<AtomicBool>) -> impl Stream<Item = Message> {
        let (tx, rx) = mpsc::channel(1);
        let initial_items = self.get_items().to_vec(); // Clone or get owned data from self

        tokio::spawn(async move {
            let cache = cache_manager::get_cache_or_default();
            let request = DataRequest::ReplicaSets {
                namespace: Some(cache_manager::get_current_namespace_or_default()),
                labels: std::collections::BTreeMap::new(),
            };

            // Subscribe to cache updates
            let (sub_id, mut cache_rx) = cache
                .subscription_manager
                .subscribe("rs:*".to_string())
                .await;

            // Start with cached data if available
            if let Some(FetchResult::ReplicaSets(cached_items)) = cache.get(&request).await {
                if !cached_items.is_empty()
                    && cached_items != initial_items
                    && tx.send(Message::Rs(cached_items)).await.is_err()
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
                        if let Some(crate::k8s::cache::DataUpdate::ReplicaSets(new_items)) = update {
                            // IMMEDIATE PREFETCH: Trigger Pod fetching for subscription updates too
                            trigger_replicaset_pod_prefetch(&new_items, "UPDATE").await;

                            if !new_items.is_empty() && new_items != initial_items && tx.send(Message::Rs(new_items)).await.is_err() {
                                break;
                            }
                        }
                    }
                    // Fallback: check cache periodically and refresh if needed
                    () = sleep(Duration::from_millis(POLL_MS)) => {
                        // Try cache first

                     if let Some(FetchResult::ReplicaSets(cached_items)) = cache.get(&request).await {
                         debug!("⚡ Using cached ReplicaSets data ({} items)", cached_items.len());
                         // IMMEDIATE PREFETCH: Trigger Pod fetching for cached ReplicaSets too
                         trigger_replicaset_pod_prefetch(&cached_items, "CACHED").await;

                         if !cached_items.is_empty() && cached_items != initial_items && tx.send(Message::Rs(cached_items)).await.is_err() {
                             break;
                         }
                     } else {
                         // Cache miss - fall back to direct API call
                         warn!("🌐 API FALLBACK: ReplicaSets cache miss, calling K8s API");
                         match list_replicas().await {
                             Ok(new_items) => {
                                 if !new_items.is_empty() {
                                     // Store in cache for next time
                                     let fetch_result = FetchResult::ReplicaSets(new_items.clone());
                                     let _ = cache.put(&request, fetch_result).await;

                                     // IMMEDIATE PREFETCH: Trigger Pod fetching for visible ReplicaSets
                                     trigger_replicaset_pod_prefetch(&new_items, "IMMEDIATE").await;

                                     if new_items != initial_items && tx.send(Message::Rs(new_items)).await.is_err() {
                                         break;
                                     }
                                 }
                             }
                             Err(_e) => {

                                 // Still try to use stale cache data
                                 if let Some(FetchResult::ReplicaSets(stale_items)) = cache.get_or_mark_stale(&request).await {
                                     if !stale_items.is_empty() && stale_items != initial_items && tx.send(Message::Rs(stale_items)).await.is_err() {
                                         break;
                                     }
                                 }
                             }
                         }
                     }
                    }
                }
            }

            // Cleanup subscription
            cache.subscription_manager.unsubscribe(&sub_id).await;
        });

        ReceiverStream::new(rx)
    }
}

impl App {
    pub fn new(data_vec: Vec<Rs>) -> Self {
        Self {
            state: TableState::default().with_selected(0),
            scroll_state: ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT),
            colors: TableColors::new(&PALETTES[0]),
            color_index: 0,
            items: data_vec,
            filter: String::new(),
            show_filter_edit: false,
            edit_filter_cursor_position: 0,
        }
    }

    fn handle_filter_edit_event(&mut self, event: &Message) -> Option<Apps> {
        let mut app_holder = Some(Apps::Rs { app: self.clone() });
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Backspace, Char, Enter, Esc, Left, Right};

                    match key.code {
                        Char(to_insert) => {
                            self.enter_char(to_insert);
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        Backspace => {
                            self.delete_char();
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        Left => {
                            self.move_cursor_left();
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        Right => {
                            self.move_cursor_right();
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        Esc | Enter => {
                            self.set_show_filter_edit(false);
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        _ => {}
                    }
                }
            }
            Message::Rs(data_vec) => {
                debug!("updating rs app data...");
                let new_app = Self {
                    scroll_state: ScrollbarState::new(
                        data_vec.len().saturating_sub(1) * ITEM_HEIGHT,
                    ),
                    items: data_vec.clone(),
                    ..self.clone()
                };
                let new_app_holder = Apps::Rs { app: new_app };
                app_holder = Some(new_app_holder);
            }
            _ => {}
        }
        app_holder
    }

    async fn handle_table_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        let mut app_holder = Some(Apps::Rs { app: self.clone() });
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Char, Down, Enter, Up};

                    match key.code {
                        Char('q') => {
                            app_holder = None;
                            debug!("quitting...");
                        }
                        Char('j') | Down => {
                            self.next();
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        Char('k') | Up => {
                            self.previous();
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        Char('c' | 'C') => {
                            self.next_color();
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        Char('e' | 'E') => {
                            let new_app_holder = Apps::Event {
                                app: event_app::app::App::new(),
                            };
                            app_holder = Some(new_app_holder);
                            debug!("changing app from rs to event...");
                        }
                        Char('i' | 'I') => {
                            if let Some(selection) = self.get_selected_item() {
                                if let Some(selector) = selection.selectors.clone() {
                                    let data_vec =
                                        create_ingress_data_vec(selector.clone()).await?;
                                    let new_app_holder = Apps::Ingress {
                                        app: ingress_app::app::App::new(data_vec),
                                    };
                                    app_holder = Some(new_app_holder);
                                    debug!("changing app from rs to ingress...");
                                }
                            }
                        }
                        Char('f' | 'F') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_forward();
                        }
                        Char('b' | 'B') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_backward();
                        }
                        Enter => {
                            if let Some(selection) = self.get_selected_item() {
                                if let Some(selectors) = selection.selectors.clone() {
                                    let data_vec = vec![];
                                    let new_app_holder = Apps::Pod {
                                        app: pod_app::app::App::new(selectors, data_vec),
                                    };
                                    app_holder = Some(new_app_holder);
                                    debug!("changing app from rs to pod...");
                                }
                            }
                        }
                        Char('/') => {
                            self.set_show_filter_edit(true);
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        _k => {}
                    }
                }
            }
            Message::Rs(data_vec) => {
                debug!("updating rs app data...");
                let new_app = Self {
                    scroll_state: ScrollbarState::new(
                        data_vec.len().saturating_sub(1) * ITEM_HEIGHT,
                    ),
                    items: data_vec.clone(),
                    ..self.clone()
                };
                let new_app_holder = Apps::Rs { app: new_app };
                app_holder = Some(new_app_holder);
            }
            _ => {}
        }
        Ok(app_holder)
    }

    pub const fn set_cursor_pos(&mut self, cursor_pos: usize) {
        self.edit_filter_cursor_position = cursor_pos;
    }
    #[allow(clippy::missing_const_for_fn)]
    pub fn get_cursor_pos(&self) -> usize {
        self.edit_filter_cursor_position
    }

    pub const fn set_show_filter_edit(&mut self, show_filter_edit: bool) {
        self.show_filter_edit = show_filter_edit;
    }
    #[allow(clippy::missing_const_for_fn)]
    pub fn get_show_filter_edit(&self) -> bool {
        self.show_filter_edit
    }

    pub fn get_event_details(&mut self) -> Vec<(String, String, Option<String>)> {
        self.get_selected_item().map_or_else(Vec::new, |pod| {
            pod.events
                .iter()
                .map(|event| {
                    (
                        event.type_.clone(),
                        event.message.clone(),
                        Some(event.age.clone()),
                    )
                })
                .collect()
        })
    }

    pub fn get_left_details(&mut self) -> Vec<(String, String, Option<String>)> {
        self.get_selected_item().map_or_else(Vec::new, |pod| {
            pod.selectors.clone().map_or_else(Vec::new, |labels| {
                let mut r = Vec::new();
                for (name, value) in &labels {
                    r.push((name.to_string(), value.to_string(), None));
                }
                r
            })
        })
    }
}
