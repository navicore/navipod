use crate::k8s::rs::list_replicas;
use crate::tui::data::{rs_constraint_len_calculator, Rs};
use crate::tui::ingress_app;
use crate::tui::pod_app;
use crate::tui::rs_app::ui;
use crate::tui::stream::Message;
use crate::tui::style::{TableColors, ITEM_HEIGHT, PALETTES};
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{create_ingress_data_vec, AppBehavior, Apps};
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
    pub(crate) items: Vec<Rs>,
    pub(crate) longest_item_lens: (u16, u16, u16, u16, u16),
    pub(crate) scroll_state: ScrollbarState,
    pub(crate) colors: TableColors,
    pub(crate) color_index: usize,
    pub(crate) table_height: usize,
    pub(crate) filter: String,
    pub(crate) show_filter_edit: bool,
    pub(crate) edit_filter_cursor_position: usize,
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

impl AppBehavior for App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        if self.get_show_filter_edit() {
            Ok(self.handle_filter_edit_event(event))
        } else {
            self.handle_table_event(event).await
        }
    }

    fn draw_ui<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal.draw(|f| ui::ui(f, self))?; // Pass self directly if mutable access is not required
        Ok(())
    }

    fn stream(&self, should_stop: Arc<AtomicBool>) -> impl Stream<Item = Message> {
        let (tx, rx) = mpsc::channel(1);
        let initial_items = self.get_items().to_vec(); // Clone or get owned data from self

        tokio::spawn(async move {
            while !should_stop.load(Ordering::Relaxed) {
                match list_replicas().await {
                    Ok(new_items) => {
                        if !new_items.is_empty() && new_items != initial_items {
                            let sevent = Message::Rs(new_items);
                            if tx.send(sevent).await.is_err() {
                                break;
                            }
                        }
                        sleep(Duration::from_millis(POLL_MS)).await;
                    }
                    Err(_e) => {
                        break;
                    }
                };
            }
        });

        ReceiverStream::new(rx)
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
            filter: String::new(),
            show_filter_edit: false,
            edit_filter_cursor_position: 0,
        }
    }

    fn handle_filter_edit_event(&mut self, event: &Message) -> Option<Apps> {
        let mut app_holder = Some(Apps::Rs { app: self.clone() });
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::{Backspace, Char, Enter, Esc, Left, Right};

                    match key.code {
                        Char(to_insert) => {
                            self.enter_char(to_insert);
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        Backspace => {
                            self.delete_char();
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        Left => {
                            self.move_cursor_left();
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        Right => {
                            self.move_cursor_right();
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        Esc | Enter => {
                            self.set_show_filter_edit(false);
                            app_holder = Some(Apps::Rs { app: self.clone() });
                        }
                        _ => {}
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
        app_holder
    }

    async fn handle_table_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
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
                        Char('/') => {
                            self.set_show_filter_edit(true);
                            app_holder = Some(Apps::Rs { app: self.clone() });
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

    // fn move_cursor_left(&mut self) {
    //     let cursor_moved_left = self.get_cursor_pos().saturating_sub(1);
    //     self.set_cursor_pos(self.clamp_cursor(cursor_moved_left));
    // }
    //
    // fn move_cursor_right(&mut self) {
    //     let cursor_moved_right = self.get_cursor_pos().saturating_add(1);
    //     self.set_cursor_pos(self.clamp_cursor(cursor_moved_right));
    // }
    //
    // fn enter_char(&mut self, new_char: char) {
    //     let mut f = self.get_filter();
    //     f.insert(self.get_cursor_pos(), new_char);
    //     self.set_filter(f);
    //
    //     self.move_cursor_right();
    // }
    //
    // fn delete_char(&mut self) {
    //     let is_not_cursor_leftmost = self.get_cursor_pos() != 0;
    //     if is_not_cursor_leftmost {
    //         let current_index = self.get_cursor_pos();
    //         let from_left_to_current_index = current_index - 1;
    //
    //         let f = self.get_filter();
    //         let before_char_to_delete = f.chars().take(from_left_to_current_index);
    //         let f = self.get_filter();
    //         let c = f.chars();
    //         let after_char_to_delete = c.skip(current_index);
    //
    //         self.set_filter(before_char_to_delete.chain(after_char_to_delete).collect());
    //         self.move_cursor_left();
    //     }
    // }
    //
    // fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
    //     new_cursor_pos.clamp(0, self.filter.len())
    // }
    //
    // fn _reset_cursor(&mut self) {
    //     self.set_cursor_pos(0);
    // }
    //
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
}
