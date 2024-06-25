use crate::tui::cert_app;
use crate::tui::data::{cert_constraint_len_calculator, Cert};
use crate::tui::stream::Message;
use crate::tui::style::{TableColors, ITEM_HEIGHT, PALETTES};
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{AppBehavior, Apps};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use futures::{stream, Stream};
use ratatui::prelude::*;
use ratatui::widgets::{ScrollbarState, TableState};
use std::io;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct App {
    pub(crate) state: TableState,
    pub(crate) items: Vec<Cert>,
    pub(crate) longest_item_lens: (u16, u16, u16, u16),
    pub(crate) scroll_state: ScrollbarState,
    pub(crate) colors: TableColors,
    pub(crate) color_index: usize,
    pub(crate) filter: String,
}

impl TuiTableState for App {
    type Item = Cert;

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

    fn set_cursor_pos(&mut self, _cursor_pos: usize) {
        todo!()
    }

    fn get_cursor_pos(&self) -> usize {
        todo!()
    }

    fn set_show_filter_edit(&mut self, _show_filter_edit: bool) {
        todo!()
    }

    fn get_show_filter_edit(&self) -> bool {
        todo!()
    }
}

impl AppBehavior for cert_app::app::App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        let mut app_holder = Some(Apps::Cert { app: self.clone() });
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Char, Down, Esc, Up};
                    match key.code {
                        Char('q') | Esc => {
                            app_holder = None;
                        }
                        Char('j') | Down => {
                            self.next();
                            //todo: stop all this cloning
                            app_holder = Some(Apps::Cert { app: self.clone() });
                        }
                        Char('k') | Up => {
                            self.previous();
                            app_holder = Some(Apps::Cert { app: self.clone() });
                        }
                        Char('c' | 'C') => {
                            self.next_color();
                            app_holder = Some(Apps::Cert { app: self.clone() });
                        }
                        Char('f' | 'F') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_forward();
                        }
                        Char('b' | 'B') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_backward();
                        }
                        _k => {}
                    }
                }
            }
            Message::Cert(data_vec) => {
                let new_app = Self {
                    longest_item_lens: cert_constraint_len_calculator(data_vec),
                    scroll_state: ScrollbarState::new(
                        data_vec.len().saturating_sub(1) * ITEM_HEIGHT,
                    ),
                    items: data_vec.clone(),
                    ..self.clone()
                };
                let new_app_holder = Apps::Cert { app: new_app };
                app_holder = Some(new_app_holder);
            }
            _ => {}
        }
        Ok(app_holder)
    }
    fn draw_ui<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal.draw(|f| cert_app::ui::ui(f, &mut self.clone()))?;
        Ok(())
    }

    fn stream(&self, _should_stop: Arc<AtomicBool>) -> impl Stream<Item = Message> {
        stream::empty()
    }
}

impl App {
    pub fn new(data_vec: Vec<Cert>) -> Self {
        Self {
            state: TableState::default().with_selected(0),
            longest_item_lens: cert_constraint_len_calculator(&data_vec),
            scroll_state: ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT),
            colors: TableColors::new(&PALETTES[0]),
            color_index: 1,
            items: data_vec,
            filter: String::new(),
        }
    }
}
