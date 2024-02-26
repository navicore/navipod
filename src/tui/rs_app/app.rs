use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::widgets::{ScrollbarState, TableState};
use std::io;
use tracing::debug;

use crate::tui::data::{rs_constraint_len_calculator, Rs};
use crate::tui::ingress_app;
use crate::tui::pod_app;
use crate::tui::rs_app::ui;
use crate::tui::stream::Message;
use crate::tui::style::{TableColors, ITEM_HEIGHT, PALETTES};
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{create_ingress_data_vec, AppBehavior, Apps};
use ratatui::prelude::*;

#[derive(Clone, Debug)]
pub struct App {
    pub(crate) state: TableState,
    pub(crate) items: Vec<Rs>,
    pub(crate) longest_item_lens: (u16, u16, u16, u16, u16),
    pub(crate) scroll_state: ScrollbarState,
    pub(crate) colors: TableColors,
    pub(crate) color_index: usize,
    pub(crate) table_height: usize,
}

impl TuiTableState for App {
    type Item = Rs;

    fn get_items(&self) -> &[Self::Item] {
        &self.items
    }

    fn get_state(&mut self) -> &mut TableState {
        &mut self.state
    }

    fn set_state(&mut self, state: TableState) {
        self.state = state;
    }

    fn get_scroll_state(&self) -> &ScrollbarState {
        &self.scroll_state
    }

    fn set_scroll_state(&mut self, scroll_state: ScrollbarState) {
        self.scroll_state = scroll_state;
    }

    fn get_table_colors(&self) -> &TableColors {
        &self.colors
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

    fn get_table_height(&self) -> usize {
        self.table_height
    }

    fn set_table_height(&mut self, table_height: usize) {
        self.table_height = table_height;
    }

    fn reset_selection_state(&mut self) {
        self.state = TableState::default().with_selected(0);
        self.scroll_state = ScrollbarState::new(self.items.len().saturating_sub(1) * ITEM_HEIGHT);
    }
}

impl AppBehavior for App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        let mut app_holder = Some(Apps::Rs { app: self.clone() });
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Char, Down, Enter, Up};
                    match key.code {
                        Char('q') => {
                            app_holder = None;
                            debug!("quiting...");
                        }
                        Char('j') | Down => {
                            self.next();
                            //todo: stop all this cloning
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
                                };
                            };
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
                                };
                            };
                        }
                        _k => {}
                    }
                }
            }
            Message::Rs(data_vec) => {
                debug!("updating rs app data...");
                let new_app = Self {
                    longest_item_lens: rs_constraint_len_calculator(data_vec),
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

    fn draw_ui<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal.draw(|f| ui::ui(f, self))?; // Pass self directly if mutable access is not required
        Ok(())
    }
}

impl App {
    pub fn new(data_vec: Vec<Rs>) -> Self {
        Self {
            state: TableState::default().with_selected(0),
            longest_item_lens: rs_constraint_len_calculator(&data_vec),
            scroll_state: ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT),
            colors: TableColors::new(&PALETTES[0]),
            color_index: 0,
            table_height: 0,
            items: data_vec,
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

    // pub fn get_right_details(&mut self) -> Vec<ResourcceLabel> {
    //     vec![]
    // }
}
