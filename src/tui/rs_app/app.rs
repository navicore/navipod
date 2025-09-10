use crate::cache_manager;
use crate::impl_tui_table_state;
use crate::k8s::cache::{DataRequest, FetchResult};
use crate::k8s::rs::list_replicas;
use crate::tui::common::base_table_state::BaseTableState;
use crate::tui::common::key_handler::{handle_common_keys, KeyHandlerResult};
use crate::tui::data::Rs;
use crate::tui::pod_app;
// use crate::tui::rs_app::ui; // Unused while testing modern UI
use crate::tui::stream::Message;
use crate::tui::style::ITEM_HEIGHT;
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{create_ingress_data_vec, AppBehavior, Apps};
use crate::tui::yaml_editor::YamlEditor;
use crate::tui::rs_app::domain::ReplicaSetDomainService;
use crate::tui::{event_app, ingress_app};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use futures::Stream;
use ratatui::prelude::*;
use ratatui::widgets::ScrollbarState;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, warn};

const POLL_MS: u64 = 5000;


#[derive(Clone, Debug)]
pub struct App {
    pub(crate) base: BaseTableState<Rs>,
    /// Track network activity for UI indicator
    pub(crate) has_network_activity: bool,
    /// Track blocking activity (cache misses) for red spinner
    pub(crate) has_blocking_activity: bool,
}

impl_tui_table_state!(App, Rs);

impl AppBehavior for App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        // Handle YAML editor events first if editor is active
        if self.base.yaml_editor.is_active {
            return self.handle_yaml_editor_event(event);
        }

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
                            ReplicaSetDomainService::trigger_pod_prefetch(&new_items, "UPDATE").await;

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
                         ReplicaSetDomainService::trigger_pod_prefetch(&cached_items, "CACHED").await;

                         if !cached_items.is_empty() && cached_items != initial_items && tx.send(Message::Rs(cached_items)).await.is_err() {
                             break;
                         }
                     } else {
                         // Cache miss - fall back to direct API call with blocking activity tracking
                         warn!("🔴 CACHE MISS: ReplicaSets cache miss, calling K8s API (blocking)");
                         cache_manager::start_blocking_operation();
                         let api_result = list_replicas().await;
                         cache_manager::end_blocking_operation();
                         match api_result {
                             Ok(new_items) => {
                                 if !new_items.is_empty() {
                                     // Store in cache for next time
                                     let fetch_result = FetchResult::ReplicaSets(new_items.clone());
                                     let _ = cache.put(&request, fetch_result).await;

                                     // IMMEDIATE PREFETCH: Trigger Pod fetching for visible ReplicaSets
                                     ReplicaSetDomainService::trigger_pod_prefetch(&new_items, "IMMEDIATE").await;

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
            base: BaseTableState::new(data_vec),
            has_network_activity: false,
            has_blocking_activity: false,
        }
    }

    /// Update activity status for UI indicator
    pub fn update_activity_status(&mut self) {
        self.has_network_activity = cache_manager::has_network_activity();
        self.has_blocking_activity = cache_manager::has_blocking_activity();
    }

    /// Get current network activity status
    pub const fn get_network_activity(&self) -> bool {
        self.has_network_activity
    }

    /// Get current blocking activity status (cache misses - should be red!)
    pub const fn get_blocking_activity(&self) -> bool {
        self.has_blocking_activity
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
                let mut new_app = self.clone();
                new_app.base.items.clone_from(data_vec);
                new_app.base.scroll_state =
                    ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT);
                let new_app_holder = Apps::Rs { app: new_app };
                app_holder = Some(new_app_holder);
            }
            _ => {}
        }
        app_holder
    }

    async fn handle_table_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    // First try common keys (navigation, quit, color, vim motions)
                    return match handle_common_keys(self, key, |app| Apps::Rs { app }) {
                        KeyHandlerResult::Quit => Ok(None),
                        KeyHandlerResult::HandledWithUpdate(app_holder) | KeyHandlerResult::Handled(app_holder) => Ok(app_holder),
                        KeyHandlerResult::NotHandled => {
                            // Handle RS-specific keys
                            self.handle_rs_specific_keys(key).await
                        }
                    };
                }
                Ok(Some(Apps::Rs { app: self.clone() }))
            }
            Message::Rs(data_vec) => {
                Ok(Some(self.handle_data_update(data_vec)))
            }
            _ => Ok(Some(Apps::Rs { app: self.clone() })),
        }
    }

    /// Handle RS-specific key events that aren't covered by common key handler
    async fn handle_rs_specific_keys(&mut self, key: &crossterm::event::KeyEvent) -> Result<Option<Apps>, io::Error> {
        use KeyCode::{Char, Enter};
        
        match key.code {
            Char('e') => {
                debug!("changing app from rs to event...");
                Ok(Some(Self::handle_switch_to_events()))
            }
            Char('i' | 'I') => self.handle_switch_to_ingress().await,
            Enter => Ok(Some(self.handle_switch_to_pods())),
            Char('/') => Ok(Some(self.handle_filter_mode())),
            Char('y' | 'Y') => Ok(Some(self.handle_yaml_view())),
            _ => Ok(Some(Apps::Rs { app: self.clone() })),
        }
    }

    /// Handle data update message
    fn handle_data_update(&self, data_vec: &[Rs]) -> Apps {
        debug!("updating rs app data...");
        let mut new_app = self.clone();
        new_app.base.items = data_vec.to_vec();
        new_app.base.scroll_state =
            ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT);
        
        // Update network activity status for UI indicator
        new_app.update_activity_status();
        
        Apps::Rs { app: new_app }
    }

    /// Switch to Events app
    fn handle_switch_to_events() -> Apps {
        Apps::Event {
            app: event_app::app::App::new(),
        }
    }

    /// Switch to Ingress app
    async fn handle_switch_to_ingress(&mut self) -> Result<Option<Apps>, io::Error> {
        if let Some(selection) = self.get_selected_item() {
            if let Some(selector) = selection.selectors.clone() {
                let data_vec = create_ingress_data_vec(selector.clone()).await?;
                debug!("changing app from rs to ingress...");
                return Ok(Some(Apps::Ingress {
                    app: ingress_app::app::App::new(data_vec),
                }));
            }
        }
        Ok(Some(Apps::Rs { app: self.clone() }))
    }

    /// Switch to Pods app
    fn handle_switch_to_pods(&mut self) -> Apps {
        if let Some(selection) = self.get_selected_item() {
            if let Some(selectors) = selection.selectors.clone() {
                let data_vec = vec![];
                debug!("changing app from rs to pod...");
                return Apps::Pod {
                    app: pod_app::app::App::new(selectors, data_vec),
                };
            }
        }
        Apps::Rs { app: self.clone() }
    }

    /// Enter filter editing mode
    fn handle_filter_mode(&mut self) -> Apps {
        self.set_show_filter_edit(true);
        Apps::Rs { app: self.clone() }
    }

    /// View YAML for selected `ReplicaSet`
    fn handle_yaml_view(&mut self) -> Apps {
        if let Some(selection) = self.get_selected_item() {
            self.base.yaml_editor = YamlEditor::new(
                "replicaset".to_string(),
                selection.name.clone(),
                Some(cache_manager::get_current_namespace_or_default()),
            );
            if let Err(e) = self.base.yaml_editor.fetch_yaml() {
                debug!("Error fetching YAML: {}", e);
            }
        }
        Apps::Rs { app: self.clone() }
    }

    pub const fn set_cursor_pos(&mut self, cursor_pos: usize) {
        self.base.edit_filter_cursor_position = cursor_pos;
    }
    pub const fn get_cursor_pos(&self) -> usize {
        self.base.edit_filter_cursor_position
    }

    pub const fn set_show_filter_edit(&mut self, show_filter_edit: bool) {
        self.base.show_filter_edit = show_filter_edit;
    }
    pub const fn get_show_filter_edit(&self) -> bool {
        self.base.show_filter_edit
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

    /// Handle YAML editor events
    fn handle_yaml_editor_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        if let Message::Key(Event::Key(key)) = event {
            if key.kind == KeyEventKind::Press {
                use KeyCode::{Char, Down, Esc, Up};

                match key.code {
                    Char('q') | Esc => {
                        // Close YAML editor
                        self.base.yaml_editor.close();
                    }
                    Char('r' | 'R') => {
                        // Refresh YAML content
                        self.base.yaml_editor.fetch_yaml()?;
                    }
                    // Removed mode switching - now read-only viewer only
                    Up | Char('k') => {
                        // Scroll up (vim-like navigation)
                        self.base.yaml_editor.scroll_up(3);
                    }
                    Down | Char('j') => {
                        // Scroll down (vim-like navigation)
                        self.base.yaml_editor.scroll_down(3, None); // Use dynamic height calculation
                    }
                    Char('G') => {
                        // Jump to bottom (vim motion)
                        self.base.yaml_editor.jump_to_bottom(None); // Use dynamic height calculation
                    }
                    Char('g') => {
                        // Jump to top (vim motion)
                        self.base.yaml_editor.jump_to_top();
                    }
                    _ => {}
                }
            }
        }

        Ok(Some(Apps::Rs { app: self.clone() }))
    }
}
