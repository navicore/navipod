use crate::k8s::pods::list_rspods;
use crate::tui::container_app;
use crate::tui::data::{RsPod, pod_constraint_len_calculator};
use crate::tui::ingress_app;
use crate::tui::pod_app;
use crate::tui::stream::Message;
use crate::tui::style::{ITEM_HEIGHT, PALETTES, TableColors};
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{AppBehavior, Apps, create_container_data_vec, create_ingress_data_vec};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use futures::Stream;
use ratatui::prelude::*;
use ratatui::widgets::{ScrollbarState, TableState};
use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_stream::wrappers::ReceiverStream;
use tracing::debug;

const POLL_MS: u64 = 5000;

#[derive(Clone, Debug)]
pub struct App {
    pub(crate) state: TableState,
    pub(crate) items: Vec<RsPod>,
    pub(crate) longest_item_lens: (u16, u16, u16, u16, u16),
    pub(crate) scroll_state: ScrollbarState,
    pub(crate) colors: TableColors,
    pub(crate) color_index: usize,
    pub(crate) selector: BTreeMap<String, String>,
    pub(crate) filter: String,
}

impl TuiTableState for App {
    type Item = RsPod;

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

impl AppBehavior for pod_app::app::App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        let mut app_holder = Some(Apps::Pod { app: self.clone() });
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
                        _k => {}
                    }
                }
            }
            Message::Pod(data_vec) => {
                debug!("updating pod app data...");
                let new_app = Self {
                    longest_item_lens: pod_constraint_len_calculator(data_vec),
                    items: data_vec.clone(),
                    scroll_state: ScrollbarState::new(
                        data_vec.len().saturating_sub(1) * ITEM_HEIGHT,
                    ),
                    ..self.clone()
                };
                let new_app_holder = Apps::Pod { app: new_app };
                app_holder = Some(new_app_holder);
            }
            _ => {}
        }
        Ok(app_holder)
    }
    fn draw_ui<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal.draw(|f| pod_app::ui::ui(f, &mut self.clone()))?;
        Ok(())
    }

    fn stream(&self, should_stop: Arc<AtomicBool>) -> impl Stream<Item = Message> {
        let (tx, rx) = mpsc::channel(100);

        let initial_items = self.get_items().to_vec();
        let selector = self.selector.clone();

        tokio::spawn(async move {
            while !should_stop.load(Ordering::Relaxed) {
                //get Vec and send
                match list_rspods(selector.clone()).await {
                    Ok(d) => {
                        if !d.is_empty() && d != initial_items {
                            let sevent = Message::Pod(d);
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
            }
        });

        ReceiverStream::new(rx)
    }
}

impl App {
    pub fn new(selector: BTreeMap<String, String>, data_vec: Vec<RsPod>) -> Self {
        Self {
            state: TableState::default().with_selected(0),
            longest_item_lens: pod_constraint_len_calculator(&data_vec),
            scroll_state: ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT),
            colors: TableColors::new(&PALETTES[0]),
            color_index: 1,
            items: data_vec,
            selector,
            filter: String::new(),
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
