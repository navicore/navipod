use crate::impl_tui_table_state;
use crate::tui::common::base_table_state::BaseTableState;
use crate::tui::container_app;
use crate::tui::data::RsPod;
use crate::tui::ingress_app;
use crate::tui::pod_app;
use crate::tui::stream::Message;
use crate::tui::style::ITEM_HEIGHT;
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{create_container_data_vec, create_ingress_data_vec, AppBehavior, Apps};
use crate::tui::yaml_editor::YamlEditor;
use crate::{
    cache_manager,
    k8s::{
        cache::{DataRequest, FetchResult, PodSelector},
        pods::list_rspods,
    },
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use futures::Stream;
use ratatui::prelude::*;
use ratatui::widgets::ScrollbarState;
use std::collections::BTreeMap;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_stream::wrappers::ReceiverStream;
use tracing::debug;

const POLL_MS: u64 = 1000;

#[derive(Clone, Debug)]
pub struct App {
    pub(crate) base: BaseTableState<RsPod>,
    pub(crate) selector: BTreeMap<String, String>,
}

impl_tui_table_state!(App, RsPod);

impl AppBehavior for pod_app::app::App {
    #[allow(clippy::too_many_lines)]
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        let mut app_holder = Some(Apps::Pod { app: self.clone() });
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Char, Down, Enter, Esc, Up};

                    // Handle YAML editor events first if active
                    if self.base.yaml_editor.is_active {
                        match key.code {
                            Char('q') | Esc => {
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
                                self.base.yaml_editor = YamlEditor::new(
                                    "pod".to_string(),
                                    selection.name.clone(),
                                    Some(cache_manager::get_current_namespace_or_default()),
                                );
                                if let Err(e) = self.base.yaml_editor.fetch_yaml() {
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
                let mut new_app = self.clone();
                new_app.base.items.clone_from(data_vec);
                new_app.base.scroll_state =
                    ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT);
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
                if !cached_items.is_empty()
                    && cached_items != initial_items
                    && tx.send(Message::Pod(cached_items)).await.is_err()
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
            base: BaseTableState::new(data_vec),
            selector,
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
