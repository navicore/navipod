use crate::impl_tui_table_state;
use crate::k8s::containers::logs;
use crate::tui::common::base_table_state::BaseTableState;
use crate::tui::data::LogRec;
use crate::tui::log_app;
use crate::tui::stream::Message;
use crate::tui::style::ITEM_HEIGHT;
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{AppBehavior, Apps};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use futures::Stream;
use ratatui::prelude::*;
use ratatui::widgets::ScrollbarState;
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
    pub(crate) base: BaseTableState<LogRec>,
    pub(crate) selector: BTreeMap<String, String>,
    pub(crate) pod_name: String,
    pub(crate) container_name: String,
}

impl_tui_table_state!(App, LogRec);

impl AppBehavior for log_app::app::App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        if self.get_show_filter_edit() {
            Ok(self.handle_filter_edit_event(event))
        } else {
            Ok(self.handle_table_event(event))
        }
    }

    fn draw_ui<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal.draw(|f| super::modern_ui::ui(f, self))?; // Use modern UI
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
                        if !d.is_empty() && d != initial_items && tx.send(Message::Log(d)).await.is_err() {
                            break;
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
            base: BaseTableState::new(data_vec),
            selector,
            pod_name,
            container_name,
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
                        Char('G') => {
                            // Jump to bottom (vim motion)
                            self.jump_to_bottom();
                            app_holder = Some(Apps::Log { app: self.clone() });
                        }
                        Char('g') => {
                            // Jump to top (vim motion)
                            self.jump_to_top();
                            app_holder = Some(Apps::Log { app: self.clone() });
                        }
                        _k => {}
                    }
                }
            }
            Message::Log(data_vec) => {
                let mut new_app = self.clone();
                new_app.base.items.clone_from(data_vec);
                new_app.base.scroll_state =
                    ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT);
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
                let mut new_app = self.clone();
                new_app.base.items.clone_from(data_vec);
                new_app.base.scroll_state =
                    ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT);
                let new_app_holder = Apps::Log { app: new_app };
                app_holder = Some(new_app_holder);
            }
            _ => {}
        }
        app_holder
    }
}
