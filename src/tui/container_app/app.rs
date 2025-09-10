use crate::impl_tui_table_state;
use crate::tui::common::base_table_state::BaseTableState;
use crate::tui::common::key_handler::{handle_common_keys, KeyHandlerResult};
use crate::tui::common::stream_factory::StreamFactory;
use crate::tui::container_app;
use crate::tui::data::Container;
use crate::tui::log_app;
use crate::tui::stream::Message;
use crate::tui::style::ITEM_HEIGHT;
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{AppBehavior, Apps};
use tracing::debug;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use futures::Stream;
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
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    // First try common keys (navigation, quit, color, vim motions)
                    return match handle_common_keys(self, key, |app| Apps::Container { app }) {
                        KeyHandlerResult::Quit => Ok(None),
                        KeyHandlerResult::HandledWithUpdate(app_holder) | KeyHandlerResult::Handled(app_holder) => Ok(app_holder),
                        KeyHandlerResult::NotHandled => {
                            // Handle Container-specific keys
                            Ok(Some(self.handle_container_specific_keys(key)))
                        }
                    };
                }
                Ok(Some(Apps::Container { app: self.clone() }))
            }
            Message::Container(data_vec) => {
                let mut new_app = self.clone();
                new_app.base.items.clone_from(data_vec);
                new_app.base.scroll_state =
                    ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT);
                let new_app_holder = Apps::Container { app: new_app };
                Ok(Some(new_app_holder))
            }
            _ => Ok(Some(Apps::Container { app: self.clone() }))
        }
    }
    fn draw_ui<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal.draw(|f| super::modern_ui::ui(f, self))?; // Use modern UI
        Ok(())
    }

    fn stream(&self, _should_stop: Arc<AtomicBool>) -> impl Stream<Item = Message> {
        StreamFactory::empty()
    }
}

impl App {
    pub fn new(data_vec: Vec<Container>) -> Self {
        Self {
            base: BaseTableState::new(data_vec),
        }
    }

    /// Handle Container-specific key events that aren't covered by common key handler
    fn handle_container_specific_keys(&mut self, key: &crossterm::event::KeyEvent) -> Apps {
        use KeyCode::{Enter, Esc};
        
        match key.code {
            Esc => {
                // Navigate back to Pod page
                debug!("navigating back from container to pod...");
                let data_vec = vec![];
                Apps::Pod {
                    app: crate::tui::pod_app::app::App::new(
                        std::collections::BTreeMap::new(), 
                        data_vec
                    ),
                }
            }
            Enter => {
                if let Some(selection) = self.get_selected_item() {
                    if let Some(selectors) = selection.selectors.clone() {
                        return Apps::Log {
                            app: log_app::app::App::new(
                                selectors,
                                selection.pod_name.clone(),
                                selection.name.clone(),
                            ),
                        };
                    }
                }
                Apps::Container { app: self.clone() }
            }
            _ => Apps::Container { app: self.clone() },
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
