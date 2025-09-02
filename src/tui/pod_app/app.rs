use crate::{cache_manager, k8s::{cache::{DataRequest, FetchResult, PodSelector}, pods::list_rspods}};
use crate::tui::container_app;
use crate::tui::data::RsPod;
use crate::tui::ingress_app;
use crate::tui::pod_app;
use crate::tui::stream::Message;
use crate::tui::style::{ITEM_HEIGHT, PALETTES, TableColors};
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{AppBehavior, Apps, create_container_data_vec, create_ingress_data_vec};
use crate::tui::yaml_editor::YamlEditor;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use futures::Stream;
use ratatui::prelude::*;
use ratatui::widgets::{ScrollbarState, TableState};
use std::collections::BTreeMap;
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
    pub(crate) state: TableState,
    pub(crate) items: Vec<RsPod>,
    pub(crate) scroll_state: ScrollbarState,
    pub(crate) colors: TableColors,
    pub(crate) color_index: usize,
    pub(crate) selector: BTreeMap<String, String>,
    pub(crate) filter: String,
    pub(crate) show_filter_edit: bool,
    pub(crate) edit_filter_cursor_position: usize,
    pub yaml_editor: YamlEditor,
}

impl TuiTableState for App {
    type Item = RsPod;

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

    fn get_cursor_pos(&self) -> usize {
        self.edit_filter_cursor_position
    }

    fn set_show_filter_edit(&mut self, show_filter_edit: bool) {
        self.show_filter_edit = show_filter_edit;
    }

    fn get_show_filter_edit(&self) -> bool {
        self.show_filter_edit
    }
}

impl AppBehavior for pod_app::app::App {
    #[allow(clippy::too_many_lines)]
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        let mut app_holder = Some(Apps::Pod { app: self.clone() });
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Char, Down, Enter, Esc, Up};
                    
                    // Handle YAML editor events first if active
                    if self.yaml_editor.is_active {
                        match key.code {
                            Char('q') | Esc => {
                                self.yaml_editor.close();
                            }
                            Char('r' | 'R') => {
                                // Refresh YAML content
                                self.yaml_editor.fetch_yaml()?;
                            }
                            // Removed mode switching - now read-only viewer only
                            Up | Char('k') => {
                                // Scroll up (vim-like navigation)
                                self.yaml_editor.scroll_up(3);
                            }
                            Down | Char('j') => {
                                // Scroll down (vim-like navigation) 
                                self.yaml_editor.scroll_down(3, 20); // Approximate max height
                            }
                            Char('G') => {
                                // Jump to bottom (vim motion)
                                self.yaml_editor.jump_to_bottom(20); // Approximate max height
                            }
                            Char('g') => {
                                // Jump to top (vim motion)
                                self.yaml_editor.jump_to_top();
                            }
                            _k => {}
                        }
                        app_holder = Some(Apps::Pod { app: self.clone() });
                        return Ok(app_holder);
                    }
                    
                    match key.code {
                        Char('q') | Esc => {
                            app_holder = None;
                        }
                        Char('j') | Down => {
                            self.next();
                            app_holder = Some(Apps::Pod { app: self.clone() });
                        }
                        Char('k') | Up => {
                            self.previous();
                            app_holder = Some(Apps::Pod { app: self.clone() });
                        }
                        Char('c' | 'C') => {
                            self.next_color();
                            app_holder = Some(Apps::Pod { app: self.clone() });
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
                                let data_vec = create_container_data_vec(
                                    selectors,
                                    selection.name.clone(),
                                )
                                .await?;
                                let new_app_holder = Apps::Container {
                                    app: container_app::app::App::new(data_vec),
                                };
                                app_holder = Some(new_app_holder);
                                }
                            }
                        }
                        Char('y' | 'Y') => {
                            // View YAML
                            if let Some(selection) = self.get_selected_item() {
                                self.yaml_editor = YamlEditor::new(
                                    "pod".to_string(), 
                                    selection.name.clone(),
                                    Some(cache_manager::get_current_namespace_or_default())
                                );
                                if let Err(e) = self.yaml_editor.fetch_yaml() {
                                    debug!("Error fetching YAML: {}", e);
                                }
                            }
                            app_holder = Some(Apps::Pod { app: self.clone() });
                        }
                        Char('G') => {
                            // Jump to bottom (vim motion)
                            self.jump_to_bottom();
                            app_holder = Some(Apps::Pod { app: self.clone() });
                        }
                        Char('g') => {
                            // Jump to top (vim motion)
                            self.jump_to_top();
                            app_holder = Some(Apps::Pod { app: self.clone() });
                        }
                        _k => {}
                    }
                }
            }
            Message::Pod(data_vec) => {
                debug!("updating pod app data...");
                let new_app = Self {
                    items: data_vec.clone(),
                    scroll_state: ScrollbarState::new(
                        data_vec.len().saturating_sub(1) * ITEM_HEIGHT,
                    ),
                    ..self.clone()
                };
                let new_app_holder = Apps::Pod { app: new_app };
                app_holder = Some(new_app_holder);
            }
            _ => {}
        }
        Ok(app_holder)
    }
    fn draw_ui<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal.draw(|f| super::modern_ui::ui(f, self))?; // Use modern UI
        Ok(())
    }

    fn stream(&self, should_stop: Arc<AtomicBool>) -> impl Stream<Item = Message> {
        let (tx, rx) = mpsc::channel(100);

        let initial_items = self.get_items().to_vec(); // Clone or get owned data from self
        let selector = self.selector.clone();

        tokio::spawn(async move {
            let cache = cache_manager::get_cache_or_default();
            let request = DataRequest::Pods {
                namespace: cache_manager::get_current_namespace_or_default(),
                selector: PodSelector::ByLabels(selector.clone()),
            };
            
            debug!("Pod app requesting cache key: {}", request.cache_key());

            // Subscribe to cache updates
            let (sub_id, mut cache_rx) = cache
                .subscription_manager
                .subscribe("pods:*".to_string())
                .await;

            // Start with cached data if available
            if let Some(FetchResult::Pods(cached_items)) = cache.get(&request).await {
                if !cached_items.is_empty() && cached_items != initial_items && tx.send(Message::Pod(cached_items)).await.is_err() {
                    cache.subscription_manager.unsubscribe(&sub_id).await;
                    return;
                }
            }

            // Listen for cache updates or fallback to direct polling
            while !should_stop.load(Ordering::Relaxed) {
                tokio::select! {
                    // Try to get updates from cache first
                    update = cache_rx.recv() => {
                        if let Some(crate::k8s::cache::DataUpdate::Pods(new_items)) = update {
                            if !new_items.is_empty() && new_items != initial_items && tx.send(Message::Pod(new_items)).await.is_err() {
                                break;
                            }
                        }
                    }
                    // Fallback: check cache periodically and refresh if needed
                    () = sleep(Duration::from_millis(POLL_MS)) => {
                        // Try cache first
                        debug!("updating pod app data...");
                        match cache.get(&request).await {
                            Some(FetchResult::Pods(cached_items)) => {
                                debug!("⚡ Using cached Pods data ({} items)", cached_items.len());
                                if !cached_items.is_empty() && cached_items != initial_items && tx.send(Message::Pod(cached_items)).await.is_err() {
                                    break;
                                }
                            }
                            Some(_) => {
                                // Wrong data type in cache - should not happen
                                debug!("⚠️ Unexpected data type in cache for Pod request");
                            }
                            None => {
                                // Cache miss - fall back to direct API call
                                debug!("🌐 API FALLBACK: Pods cache miss, calling K8s API");
                                match list_rspods(selector.clone()).await {
                                    Ok(new_items) => {
                                        if !new_items.is_empty() {
                                            // Store in cache for next time
                                            let fetch_result = FetchResult::Pods(new_items.clone());
                                            let _ = cache.put(&request, fetch_result).await;

                                            if new_items != initial_items && tx.send(Message::Pod(new_items)).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                    Err(_e) => {
                                        // Still try to use stale cache data
                                        if let Some(FetchResult::Pods(stale_items)) = cache.get_or_mark_stale(&request).await {
                                            if !stale_items.is_empty() && stale_items != initial_items && tx.send(Message::Pod(stale_items)).await.is_err() {
                                                break;
                                            }
                                        }
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
    pub fn new(selector: BTreeMap<String, String>, data_vec: Vec<RsPod>) -> Self {
        Self {
            state: TableState::default().with_selected(0),
            scroll_state: ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT),
            colors: TableColors::new(&PALETTES[0]),
            color_index: 1,
            items: data_vec,
            selector,
            filter: String::new(),
            show_filter_edit: false,
            edit_filter_cursor_position: 0,
            yaml_editor: YamlEditor::default(),
        }
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

    pub fn get_label_details(&mut self) -> Vec<(String, String, Option<String>)> {
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
