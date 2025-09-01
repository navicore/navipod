use crate::tui::cert_app;
use crate::tui::data::{Ingress, ingress_constraint_len_calculator};
use crate::tui::ingress_app;
use crate::tui::stream::Message;
use crate::tui::style::{ITEM_HEIGHT, PALETTES, TableColors};
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{AppBehavior, Apps, create_cert_data_vec};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use futures::{Stream, stream};
use ratatui::prelude::*;
use ratatui::widgets::{ScrollbarState, TableState};
use std::io;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tracing::debug;

#[derive(Clone, Debug)]
pub struct App {
    pub(crate) state: TableState,
    pub(crate) items: Vec<Ingress>,
    pub(crate) longest_item_lens: (u16, u16, u16, u16, u16),
    pub(crate) scroll_state: ScrollbarState,
    pub(crate) colors: TableColors,
    color_index: usize,
    pub(crate) filter: String,
    pub(crate) show_filter_edit: bool,
    pub(crate) edit_filter_cursor_position: usize,
}

impl TuiTableState for App {
    type Item = Ingress;

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

impl AppBehavior for ingress_app::app::App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        let mut app_holder = Some(Apps::Ingress { app: self.clone() });
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

                        _k => {}
                    }
                }
            }
            Message::Ingress(data_vec) => {
                let new_app = Self {
                    longest_item_lens: ingress_constraint_len_calculator(data_vec),
                    scroll_state: ScrollbarState::new(
                        data_vec.len().saturating_sub(1) * ITEM_HEIGHT,
                    ),
                    items: data_vec.clone(),
                    ..self.clone()
                };
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
        stream::empty()
    }
}

impl App {
    pub fn new(data_vec: Vec<Ingress>) -> Self {
        Self {
            state: TableState::default().with_selected(0),
            longest_item_lens: ingress_constraint_len_calculator(&data_vec),
            scroll_state: ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT),
            colors: TableColors::new(&PALETTES[0]),
            color_index: 3,
            items: data_vec,
            filter: String::new(),
            show_filter_edit: false,
            edit_filter_cursor_position: 0,
        }
    }
}
