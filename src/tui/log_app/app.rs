use crate::impl_tui_table_state;
use crate::k8s::containers::logs;
use crate::tui::common::base_table_state::BaseTableState;
use crate::tui::common::key_handler::{handle_common_keys, handle_filter_editing_keys, KeyHandlerResult};
use crate::tui::data::LogRec;
use crate::tui::log_app;
use crate::tui::stream::Message;
use crate::tui::style::ITEM_HEIGHT;
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{AppBehavior, Apps};
use crossterm::event::{Event, KeyCode, KeyEventKind};
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
            Ok(Some(self.handle_filter_edit_event(event)))
        } else {
            self.handle_table_event(event)
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

    /// Handle Log-specific key events that aren't covered by common key handler
    fn handle_log_specific_keys(&mut self, key: &crossterm::event::KeyEvent) -> Apps {
        use KeyCode::{Char, Enter};
        
        match key.code {
            Char('/') => {
                self.set_show_filter_edit(true);
                Apps::Log { app: self.clone() }
            }
            Enter => {
                // noop for now but will be pretty printed detail analysis popup
                Apps::Log { app: self.clone() }
            }
            _ => Apps::Log { app: self.clone() },
        }
    }

    fn handle_table_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    // Handle ESC specially to return None for history navigation
                    if key.code == KeyCode::Esc {
                        debug!("navigating back from log to container...");
                        return Ok(None); // This will use the history stack
                    }
                    
                    // First try common keys (navigation, quit, color, vim motions)
                    return match handle_common_keys(self, key, |app| Apps::Log { app }) {
                        KeyHandlerResult::Quit => Ok(None),
                        KeyHandlerResult::HandledWithUpdate(app_holder) | KeyHandlerResult::Handled(app_holder) => Ok(app_holder),
                        KeyHandlerResult::NotHandled => {
                            // Handle Log-specific keys
                            Ok(Some(self.handle_log_specific_keys(key)))
                        }
                    };
                }
                Ok(Some(Apps::Log { app: self.clone() }))
            }
            Message::Log(data_vec) => {
                let mut new_app = self.clone();
                new_app.base.items.clone_from(data_vec);
                new_app.base.scroll_state =
                    ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT);
                Ok(Some(Apps::Log { app: new_app }))
            }
            _ => Ok(Some(Apps::Log { app: self.clone() }))
        }
    }

    fn handle_filter_edit_event(&mut self, event: &Message) -> Apps {
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    if let Some(app) = handle_filter_editing_keys(self, key, |app| Apps::Log { app }) {
                        return app;
                    }
                }
                Apps::Log { app: self.clone() }
            }
            Message::Log(data_vec) => {
                debug!("updating log app data...");
                let mut new_app = self.clone();
                new_app.base.items.clone_from(data_vec);
                new_app.base.scroll_state =
                    ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT);
                Apps::Log { app: new_app }
            }
            _ => Apps::Log { app: self.clone() }
        }
    }
}
