use crate::k8s::containers::logs;
use crate::tui::data::{log_constraint_len_calculator, LogRec};
use crate::tui::log_app;
use crate::tui::stream::Message;
use crate::tui::style::{TableColors, ITEM_HEIGHT, PALETTES};
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{AppBehavior, Apps};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use futures::Stream;
use ratatui::prelude::*;
use ratatui::widgets::{ScrollbarState, TableState};
use std::collections::BTreeMap;
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
    pub(crate) items: Vec<LogRec>,
    pub(crate) longest_item_lens: (u16, u16, u16),
    pub(crate) scroll_state: ScrollbarState,
    pub(crate) colors: TableColors,
    color_index: usize,
    table_height: usize,
    pub(crate) selector: BTreeMap<String, String>,
    pub(crate) pod_name: String,
    pub(crate) container_name: String,
    pub(crate) filter: String,
    pub(crate) show_filter_edit: bool,
    pub(crate) edit_filter_cursor_position: usize,
}

impl TuiTableState for App {
    type Item = LogRec;

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

    fn reset_selection_state(&mut self) {
        self.state = TableState::default().with_selected(0);
        self.scroll_state = ScrollbarState::new(self.items.len().saturating_sub(1) * ITEM_HEIGHT);
    }
    fn get_table_height(&self) -> usize {
        self.table_height
    }

    fn set_table_height(&mut self, table_height: usize) {
        self.table_height = table_height;
    }

    fn get_filter(&self) -> String {
        self.filter.clone()
    }

    fn set_filter(&mut self, filter: String) {
        self.filter = filter;
    }
}

impl AppBehavior for log_app::app::App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        if self.get_show_filter_edit() {
            Ok(self.handle_filter_edit_event(event))
        } else {
            Ok(self.handle_table_event(event))
        }
    }

    fn draw_ui<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal.draw(|f| log_app::ui::ui(f, &mut self.clone()))?;
        Ok(())
    }

    fn stream(&self, should_stop: Arc<AtomicBool>) -> impl Stream<Item = Message> {
        let (tx, rx) = mpsc::channel(100);

        let initial_items = self.get_items().to_vec();
        //let selector = self.selector.clone();
        let pod_name = self.pod_name.clone();
        let container_name = self.container_name.clone();
        let selector = self.selector.clone();

        tokio::spawn(async move {
            while !should_stop.load(Ordering::Relaxed) {
                //get Vec and send
                match logs(selector.clone(), pod_name.clone(), container_name.clone()).await {
                    Ok(d) => {
                        if !d.is_empty() && d != initial_items {
                            let sevent = Message::Log(d);
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
    pub fn new(
        selector: BTreeMap<String, String>,
        pod_name: String,
        container_name: String,
    ) -> Self {
        let data_vec = vec![];
        Self {
            state: TableState::default().with_selected(0),
            longest_item_lens: log_constraint_len_calculator(&data_vec),
            scroll_state: ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT),
            colors: TableColors::new(&PALETTES[0]),
            color_index: 3,
            table_height: 0,
            items: data_vec,
            selector,
            pod_name,
            container_name,
            filter: String::new(),
            show_filter_edit: false,
            edit_filter_cursor_position: 0,
        }
    }

    fn handle_table_event(&mut self, event: &Message) -> Option<Apps> {
        let mut app_holder = Some(Apps::Log { app: self.clone() });
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
                            app_holder = Some(Apps::Log { app: self.clone() });
                        }
                        Char('k') | Up => {
                            self.previous();
                            app_holder = Some(Apps::Log { app: self.clone() });
                        }
                        Char('c' | 'C') => {
                            self.next_color();
                            app_holder = Some(Apps::Log { app: self.clone() });
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
                            app_holder = Some(Apps::Log { app: self.clone() });
                        }
                        _k => {}
                    }
                }
            }
            Message::Log(data_vec) => {
                let new_app = Self {
                    longest_item_lens: log_constraint_len_calculator(data_vec),
                    items: data_vec.clone(),
                    ..self.clone()
                };
                let new_app_holder = Apps::Log { app: new_app };
                app_holder = Some(new_app_holder);
            }
            _ => {}
        }
        app_holder
    }

    fn handle_filter_edit_event(&mut self, event: &Message) -> Option<Apps> {
        let mut app_holder = Some(Apps::Log { app: self.clone() });
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Backspace, Char, Enter, Esc, Left, Right};

                    match key.code {
                        Char(to_insert) => {
                            self.enter_char(to_insert);
                            app_holder = Some(Apps::Log { app: self.clone() });
                        }
                        Backspace => {
                            self.delete_char();
                            app_holder = Some(Apps::Log { app: self.clone() });
                        }
                        Left => {
                            self.move_cursor_left();
                            app_holder = Some(Apps::Log { app: self.clone() });
                        }
                        Right => {
                            self.move_cursor_right();
                            app_holder = Some(Apps::Log { app: self.clone() });
                        }
                        Esc | Enter => {
                            self.set_show_filter_edit(false);
                            app_holder = Some(Apps::Log { app: self.clone() });
                        }
                        _ => {}
                    }
                }
            }
            Message::Log(data_vec) => {
                debug!("updating log app data...");
                let new_app = Self {
                    longest_item_lens: log_constraint_len_calculator(data_vec),
                    items: data_vec.clone(),
                    ..self.clone()
                };
                let new_app_holder = Apps::Log { app: new_app };
                app_holder = Some(new_app_holder);
            }
            _ => {}
        }
        app_holder
    }

    fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.edit_filter_cursor_position.saturating_sub(1);
        self.edit_filter_cursor_position = self.clamp_cursor(cursor_moved_left);
    }

    fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.edit_filter_cursor_position.saturating_add(1);
        self.edit_filter_cursor_position = self.clamp_cursor(cursor_moved_right);
    }

    fn enter_char(&mut self, new_char: char) {
        self.filter
            .insert(self.edit_filter_cursor_position, new_char);

        self.move_cursor_right();
    }

    fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.edit_filter_cursor_position != 0;
        if is_not_cursor_leftmost {
            // Method "remove" is not used on the saved text for deleting the selected char.
            // Reason: Using remove on String works on bytes instead of the chars.
            // Using remove would require special care because of char boundaries.

            let current_index = self.edit_filter_cursor_position;
            let from_left_to_current_index = current_index - 1;

            // Getting all characters before the selected character.
            let before_char_to_delete = self.filter.chars().take(from_left_to_current_index);
            // Getting all characters after selected character.
            let after_char_to_delete = self.filter.chars().skip(current_index);

            // Put all characters together except the selected one.
            // By leaving the selected one out, it is forgotten and therefore deleted.
            self.filter = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_cursor_left();
        }
    }

    fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.filter.len())
    }

    fn _reset_cursor(&mut self) {
        self.edit_filter_cursor_position = 0;
    }

    pub fn set_cursor_pos(&mut self, cursor_pos: usize) {
        self.edit_filter_cursor_position = cursor_pos;
    }
    #[allow(clippy::missing_const_for_fn)]
    pub fn get_cursor_pos(&self) -> usize {
        self.edit_filter_cursor_position
    }

    pub fn set_show_filter_edit(&mut self, show_filter_edit: bool) {
        self.show_filter_edit = show_filter_edit;
    }
    #[allow(clippy::missing_const_for_fn)]
    pub fn get_show_filter_edit(&self) -> bool {
        self.show_filter_edit
    }
}
