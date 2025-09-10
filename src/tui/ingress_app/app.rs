use crate::impl_tui_table_state;
use crate::tui::common::base_table_state::BaseTableState;
use crate::tui::common::stream_factory::StreamFactory;
use crate::tui::data::Ingress;
use crate::tui::ingress_app;
use crate::tui::stream::Message;
use crate::tui::style::ITEM_HEIGHT;
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{create_cert_data_vec, AppBehavior, Apps};
use crate::tui::yaml_editor::YamlEditor;
use crate::{cache_manager, tui::cert_app};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use futures::Stream;
use ratatui::prelude::*;
use ratatui::widgets::ScrollbarState;
use std::io;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tracing::debug;

#[derive(Clone, Debug)]
pub struct App {
    pub(crate) base: BaseTableState<Ingress>,
}

impl_tui_table_state!(App, Ingress);

impl AppBehavior for ingress_app::app::App {
    #[allow(clippy::too_many_lines)]
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        let mut app_holder = Some(Apps::Ingress { app: self.clone() });
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
                        app_holder = Some(Apps::Ingress { app: self.clone() });
                        return Ok(app_holder);
                    }

                    match key.code {
                        Char('q') | Esc => {
                            app_holder = None;
                        }
                        Char('j') | Down => {
                            self.next();
                            //todo: stop all this cloning
                            app_holder = Some(Apps::Ingress { app: self.clone() });
                        }
                        Char('k') | Up => {
                            self.previous();
                            app_holder = Some(Apps::Ingress { app: self.clone() });
                        }
                        Char('c' | 'C') => {
                            self.next_color();
                            app_holder = Some(Apps::Ingress { app: self.clone() });
                        }
                        Char('f' | 'F') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_forward();
                        }
                        Char('b' | 'B') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_backward();
                        }
                        Enter => {
                            if let Some(selection) = self.get_selected_item() {
                                let host = &selection.host;
                                match create_cert_data_vec(&host.clone()).await {
                                    Ok(data_vec) => {
                                        let new_app_holder = Apps::Cert {
                                            app: cert_app::app::App::new(data_vec),
                                        };
                                        app_holder = Some(new_app_holder);
                                        debug!("changing app from pod to cert...");
                                    }
                                    Err(e) => {
                                        debug!("can not read certificate: {e}");
                                    }
                                }
                            }
                        }
                        Char('y' | 'Y') => {
                            // View YAML
                            if let Some(selection) = self.get_selected_item() {
                                self.base.yaml_editor = YamlEditor::new(
                                    "ingress".to_string(),
                                    selection.name.clone(),
                                    Some(cache_manager::get_current_namespace_or_default()),
                                );
                                if let Err(e) = self.base.yaml_editor.fetch_yaml() {
                                    debug!("Error fetching YAML: {}", e);
                                }
                            }
                            app_holder = Some(Apps::Ingress { app: self.clone() });
                        }
                        Char('G') => {
                            // Jump to bottom (vim motion)
                            self.jump_to_bottom();
                            app_holder = Some(Apps::Ingress { app: self.clone() });
                        }
                        Char('g') => {
                            // Jump to top (vim motion)
                            self.jump_to_top();
                            app_holder = Some(Apps::Ingress { app: self.clone() });
                        }

                        _k => {}
                    }
                }
            }
            Message::Ingress(data_vec) => {
                let mut new_app = self.clone();
                new_app.base.items.clone_from(data_vec);
                new_app.base.scroll_state =
                    ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT);
                let new_app_holder = Apps::Ingress { app: new_app };
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

    fn stream(&self, _should_stop: Arc<AtomicBool>) -> impl Stream<Item = Message> {
        StreamFactory::empty()
    }
}

impl App {
    pub fn new(data_vec: Vec<Ingress>) -> Self {
        Self {
            base: BaseTableState::new(data_vec),
        }
    }
}
