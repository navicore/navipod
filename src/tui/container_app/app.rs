use crate::impl_tui_table_state;
use crate::tui::common::base_table_state::BaseTableState;
use crate::tui::container_app;
use crate::tui::data::Container;
use crate::tui::log_app;
use crate::tui::stream::Message;
use crate::tui::style::ITEM_HEIGHT;
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{AppBehavior, Apps};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use futures::{stream, Stream};
use ratatui::prelude::*;
use ratatui::widgets::ScrollbarState;
use std::io;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct App {
    pub(crate) base: BaseTableState<Container>,
}

impl_tui_table_state!(App, Container);

impl AppBehavior for container_app::app::App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        let mut app_holder = Some(Apps::Container { app: self.clone() });
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
                            app_holder = Some(Apps::Container { app: self.clone() });
                        }
                        Char('k') | Up => {
                            self.previous();
                            app_holder = Some(Apps::Container { app: self.clone() });
                        }
                        Char('c' | 'C') => {
                            self.next_color();
                            app_holder = Some(Apps::Container { app: self.clone() });
                        }
                        Char('f' | 'F') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_forward();
                        }
                        Char('b' | 'B') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_backward();
                        }
                        Char('G') => {
                            self.jump_to_bottom();
                            app_holder = Some(Apps::Container { app: self.clone() });
                        }
                        Char('g') => {
                            self.jump_to_top();
                            app_holder = Some(Apps::Container { app: self.clone() });
                        }
                        Enter => {
                            if let Some(selection) = self.get_selected_item() {
                                if let Some(selectors) = selection.selectors.clone() {
                                    let new_app_holder = Apps::Log {
                                        app: log_app::app::App::new(
                                            selectors,
                                            selection.pod_name.clone(),
                                            selection.name.clone(),
                                        ),
                                    };
                                    app_holder = Some(new_app_holder);
                                }
                            }
                        }

                        _k => {}
                    }
                }
            }
            Message::Container(data_vec) => {
                let mut new_app = self.clone();
                new_app.base.items.clone_from(data_vec);
                new_app.base.scroll_state =
                    ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT);
                let new_app_holder = Apps::Container { app: new_app };
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
        stream::empty()
    }
}

impl App {
    pub fn new(data_vec: Vec<Container>) -> Self {
        Self {
            base: BaseTableState::new(data_vec),
        }
    }

    // pub fn get_event_details(&mut self) -> Vec<(String, String, Option<String>)> {
    //     vec![]
    // }

    pub fn get_left_details(&mut self) -> Vec<(String, String, Option<String>)> {
        self.get_selected_item().map_or_else(Vec::new, |container| {
            container
                .mounts
                .iter()
                .map(|label| (label.name.clone(), label.value.clone(), None))
                .collect()
        })
    }

    pub fn get_right_details(&mut self) -> Vec<(String, String, Option<String>)> {
        self.get_selected_item().map_or_else(Vec::new, |container| {
            container
                .envvars
                .iter()
                .map(|label| (label.name.clone(), label.value.clone(), None))
                .collect()
        })
    }
}
