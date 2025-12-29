use crate::cache_manager;
use crate::impl_tui_table_state;
use crate::tui::common::base_table_state::BaseTableState;
use crate::tui::common::stream_factory::StreamFactory;
use crate::tui::data::Namespace;
use crate::tui::namespace_app;
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
use std::sync::atomic::AtomicBool;
use tracing::{debug, error};

#[derive(Clone, Debug)]
pub struct App {
    pub(crate) base: BaseTableState<Namespace>,
}

impl_tui_table_state!(App, Namespace);

impl AppBehavior for namespace_app::app::App {
    #[allow(clippy::too_many_lines)]
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        let mut app_holder = Some(Apps::Namespace { app: self.clone() });
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Char, Down, Enter, Esc, Up};

                    // Handle filter editing mode
                    if self.get_show_filter_edit() {
                        match key.code {
                            Esc | Enter => {
                                self.set_show_filter_edit(false);
                            }
                            KeyCode::Backspace => {
                                self.delete_char();
                            }
                            Char(c) => {
                                self.enter_char(c);
                            }
                            _ => {}
                        }
                        app_holder = Some(Apps::Namespace { app: self.clone() });
                        return Ok(app_holder);
                    }

                    match key.code {
                        Char('q') => {
                            crate::tui::ui_loop::set_force_quit();
                            app_holder = None;
                        }
                        Esc => {
                            // Navigate back to ReplicaSet page without changing namespace
                            debug!("navigating back from namespace to rs (cancelled)...");
                            let data_vec = vec![];
                            let new_app_holder = Apps::Rs {
                                app: crate::tui::rs_app::app::App::new(data_vec),
                            };
                            app_holder = Some(new_app_holder);
                        }
                        Char('j') | Down => {
                            self.next();
                            app_holder = Some(Apps::Namespace { app: self.clone() });
                        }
                        Char('k') | Up => {
                            self.previous();
                            app_holder = Some(Apps::Namespace { app: self.clone() });
                        }
                        Char('c' | 'C') => {
                            self.next_color();
                            app_holder = Some(Apps::Namespace { app: self.clone() });
                        }
                        Char('f' | 'F') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_forward();
                        }
                        Char('b' | 'B') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_backward();
                        }
                        Char('/') => {
                            // Enter filter mode
                            self.set_show_filter_edit(true);
                            app_holder = Some(Apps::Namespace { app: self.clone() });
                        }
                        Enter => {
                            // Select namespace and switch to it
                            if let Some(selection) = self.get_selected_item() {
                                let selected_name = selection.name.clone();

                                // Don't switch if already on this namespace
                                if selection.is_current {
                                    debug!(
                                        "Already on namespace '{}', returning to RS",
                                        selected_name
                                    );
                                } else {
                                    debug!("Switching to namespace '{}'...", selected_name);

                                    // Switch namespace (stops watches, clears cache, starts new watches)
                                    if let Err(e) =
                                        cache_manager::switch_namespace(selected_name.clone()).await
                                    {
                                        error!("Failed to switch namespace: {}", e);
                                        // Stay on namespace picker on error
                                        app_holder = Some(Apps::Namespace { app: self.clone() });
                                        return Ok(app_holder);
                                    }
                                }

                                // Return to a fresh RS app with new namespace data
                                debug!("Returning to RS app after namespace switch");
                                let new_app_holder = Apps::Rs {
                                    app: crate::tui::rs_app::app::App::new(vec![]),
                                };
                                app_holder = Some(new_app_holder);
                            }
                        }
                        Char('G') => {
                            // Jump to bottom (vim motion)
                            self.jump_to_bottom();
                            app_holder = Some(Apps::Namespace { app: self.clone() });
                        }
                        Char('g') => {
                            // Jump to top (vim motion)
                            self.jump_to_top();
                            app_holder = Some(Apps::Namespace { app: self.clone() });
                        }
                        _k => {}
                    }
                }
            }
            Message::Namespace(data_vec) => {
                let mut new_app = self.clone();
                new_app.base.items.clone_from(data_vec);
                new_app.base.scroll_state =
                    ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT);
                let new_app_holder = Apps::Namespace { app: new_app };
                app_holder = Some(new_app_holder);
            }
            _ => {}
        }
        Ok(app_holder)
    }

    fn draw_ui<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal
            .draw(|f| super::modern_ui::ui(f, self))
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(())
    }

    fn stream(&self, _should_stop: Arc<AtomicBool>) -> impl Stream<Item = Message> {
        StreamFactory::empty()
    }
}

impl App {
    pub fn new(data_vec: Vec<Namespace>) -> Self {
        debug!("Namespace App::new called with {} items", data_vec.len());
        Self {
            base: BaseTableState::new(data_vec),
        }
    }
}
