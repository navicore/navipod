use crate::k8s::events::list_all;
use crate::tui::data::{event_constraint_len_calculator, ResourceEvent};
use crate::tui::event_app;
use crate::tui::stream::Message;
use crate::tui::style::{TableColors, ITEM_HEIGHT, PALETTES};
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{AppBehavior, Apps};
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
use tracing::debug;

const POLL_MS: u64 = 5000;

#[derive(Clone, Debug)]
pub struct App {
    pub(crate) state: TableState,
    pub(crate) items: Vec<ResourceEvent>,
    pub(crate) longest_item_lens: (u16, u16, u16, u16, u16),
    pub(crate) scroll_state: ScrollbarState,
    pub(crate) colors: TableColors,
    color_index: usize,
    pub(crate) filter: String,
    pub(crate) show_filter_edit: bool,
    pub(crate) edit_filter_cursor_position: usize,
}

impl TuiTableState for App {
    type Item = ResourceEvent;

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

impl AppBehavior for event_app::app::App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        if self.get_show_filter_edit() {
            Ok(self.handle_filter_edit_event(event))
        } else {
            Ok(self.handle_table_event(event))
        }
    }

    fn draw_ui<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal.draw(|f| event_app::ui::ui(f, &mut self.clone()))?;
        Ok(())
    }

    fn stream(&self, should_stop: Arc<AtomicBool>) -> impl Stream<Item = Message> {
        let (tx, rx) = mpsc::channel(100);

        let initial_items = self.get_items().to_vec();

        tokio::spawn(async move {
            while !should_stop.load(Ordering::Relaxed) {
                //get Vec and send
                match list_all().await {
                    Ok(d) => {
                        if !d.is_empty() && d != initial_items {
                            let sevent = Message::Event(d);
                            if tx.send(sevent).await.is_err() {
                                break;
                            }
                        }
                        sleep(Duration::from_millis(POLL_MS)).await;
                    }
                    Err(_e) => {
                        break;
                    }
                }
                sleep(Duration::from_millis(POLL_MS)).await;
            }
        });

        ReceiverStream::new(rx)
    }
}

impl App {
    pub fn new() -> Self {
        let data_vec = vec![];
        Self {
            state: TableState::default().with_selected(0),
            longest_item_lens: event_constraint_len_calculator(&data_vec),
            scroll_state: ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT),
            colors: TableColors::new(&PALETTES[0]),
            color_index: 3,
            items: data_vec,
            filter: String::new(),
            show_filter_edit: false,
            edit_filter_cursor_position: 0,
        }
    }

    fn handle_table_event(&mut self, event: &Message) -> Option<Apps> {
        let mut app_holder = Some(Apps::Event { app: self.clone() });
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
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        Char('k') | Up => {
                            self.previous();
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        Char('c' | 'C') => {
                            self.next_color();
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        Char('f' | 'F') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_forward();
                        }
                        Char('b' | 'B') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.page_backward();
                        }
                        Enter => {
                            // noop for now but will be pretty printed detail analysis popup
                        }
                        Char('/') => {
                            self.set_show_filter_edit(true);
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        _k => {}
                    }
                }
            }
            Message::Event(data_vec) => {
                let new_app = Self {
                    longest_item_lens: event_constraint_len_calculator(data_vec),
                    scroll_state: ScrollbarState::new(
                        data_vec.len().saturating_sub(1) * ITEM_HEIGHT,
                    ),
                    items: data_vec.clone(),
                    ..self.clone()
                };
                let new_app_holder = Apps::Event { app: new_app };
                app_holder = Some(new_app_holder);
            }
            _ => {}
        }
        app_holder
    }

    fn handle_filter_edit_event(&mut self, event: &Message) -> Option<Apps> {
        let mut app_holder = Some(Apps::Event { app: self.clone() });
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Backspace, Char, Enter, Esc, Left, Right};

                    match key.code {
                        Char(to_insert) => {
                            self.enter_char(to_insert);
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        Backspace => {
                            self.delete_char();
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        Left => {
                            self.move_cursor_left();
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        Right => {
                            self.move_cursor_right();
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        Esc | Enter => {
                            self.set_show_filter_edit(false);
                            app_holder = Some(Apps::Event { app: self.clone() });
                        }
                        _ => {}
                    }
                }
            }
            Message::Event(data_vec) => {
                debug!("updating event app data...");
                let new_app = Self {
                    longest_item_lens: event_constraint_len_calculator(data_vec),
                    scroll_state: ScrollbarState::new(
                        data_vec.len().saturating_sub(1) * ITEM_HEIGHT,
                    ),
                    items: data_vec.clone(),
                    ..self.clone()
                };
                let new_app_holder = Apps::Event { app: new_app };
                app_holder = Some(new_app_holder);
            }
            _ => {}
        }
        app_holder
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
