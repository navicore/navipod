#![allow(clippy::cognitive_complexity)] // UI event handling is necessarily complex
#![allow(clippy::option_if_let_else)] // if-let is often clearer
#![allow(clippy::items_after_statements)] // Local constants are fine

use crate::impl_tui_table_state;
use crate::k8s::probes::{ProbeExecutor, ProbeResult};
use crate::tui::common::base_table_state::BaseTableState;
use crate::tui::common::key_handler::{KeyHandlerResult, handle_common_keys};
use crate::tui::common::stream_factory::StreamFactory;
use crate::tui::container_app;
use crate::tui::data::Container;
use crate::tui::log_app;
use crate::tui::stream::Message;
use crate::tui::style::ITEM_HEIGHT;
use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::{AppBehavior, Apps};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use futures::Stream;
use ratatui::prelude::*;
use ratatui::widgets::ScrollbarState;
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tracing::debug;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FocusedPanel {
    ContainerList,
    Mounts,
    EnvVars,
    Probes,
}

#[derive(Clone, Debug)]
pub struct App {
    pub(crate) base: BaseTableState<Container>,
    /// Cache of probe results for each container
    pub(crate) probe_results: HashMap<String, Vec<ProbeResult>>,
    /// Currently focused panel (for Tab navigation)
    pub(crate) focused_panel: FocusedPanel,
    /// Selected index in the focused detail panel
    pub(crate) detail_selection: usize,
    /// Scroll offset for the detail panel (to handle more items than visible)
    pub(crate) detail_scroll_offset: usize,
    /// Whether to show probe execution popup
    pub(crate) show_probe_popup: bool,
    /// Current probe execution result for popup
    pub(crate) current_probe_result: Option<ProbeResult>,
    /// Scroll position in the probe popup
    pub(crate) probe_popup_scroll: usize,
    /// Track if container-specific handler handled a key
    pub(crate) key_was_handled_by_container: bool,
}

impl_tui_table_state!(App, Container);

impl AppBehavior for container_app::app::App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    // Handle Container-specific keys FIRST to intercept navigation when panels are focused
                    let container_result = self.handle_container_specific_keys(key);

                    // If container-specific handler didn't handle it, try common keys
                    if matches!(container_result, Apps::Container { .. })
                        && !self.key_was_handled_by_container
                    {
                        return match handle_common_keys(self, key, |app| Apps::Container { app }) {
                            KeyHandlerResult::Quit => Ok(None),
                            KeyHandlerResult::HandledWithUpdate(app_holder)
                            | KeyHandlerResult::Handled(app_holder) => Ok(app_holder),
                            KeyHandlerResult::NotHandled => Ok(Some(container_result)),
                        };
                    }

                    return Ok(Some(container_result));
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
            _ => Ok(Some(Apps::Container { app: self.clone() })),
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
            probe_results: HashMap::new(),
            focused_panel: FocusedPanel::ContainerList,
            detail_selection: 0,
            detail_scroll_offset: 0,
            show_probe_popup: false,
            current_probe_result: None,
            probe_popup_scroll: 0,
            key_was_handled_by_container: false,
        }
    }

    /// Handle Container-specific key events that aren't covered by common key handler
    #[allow(clippy::too_many_lines)] // UI event handling is necessarily complex
    fn handle_container_specific_keys(&mut self, key: &crossterm::event::KeyEvent) -> Apps {
        use KeyCode::{BackTab, Down, Enter, Esc, Tab, Up};

        // Reset the tracking flag
        self.key_was_handled_by_container = false;

        // Handle popup first if it's showing
        if self.show_probe_popup {
            self.key_was_handled_by_container = true;
            match key.code {
                Esc => {
                    self.show_probe_popup = false;
                    self.current_probe_result = None;
                    self.probe_popup_scroll = 0;
                }
                KeyCode::Char('q') => {
                    // Quit entire application from popup
                    self.show_probe_popup = false;
                    self.current_probe_result = None;
                    self.probe_popup_scroll = 0;
                    crate::tui::ui_loop::set_force_quit();
                    return Apps::Container { app: self.clone() };
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    // Scroll down in popup
                    if let Some(ref result) = self.current_probe_result {
                        let content_lines = self.count_probe_popup_lines(result);
                        if self.probe_popup_scroll + 1 < content_lines {
                            self.probe_popup_scroll += 1;
                        }
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    // Scroll up in popup
                    if self.probe_popup_scroll > 0 {
                        self.probe_popup_scroll -= 1;
                    }
                }
                KeyCode::Char('g') => {
                    // Go to top
                    self.probe_popup_scroll = 0;
                }
                KeyCode::Char('G') => {
                    // Go to bottom
                    if let Some(ref result) = self.current_probe_result {
                        let content_lines = self.count_probe_popup_lines(result);
                        self.probe_popup_scroll = content_lines.saturating_sub(1);
                    }
                }
                KeyCode::PageDown | KeyCode::Char('f')
                    if key.modifiers == crossterm::event::KeyModifiers::CONTROL =>
                {
                    // Page down
                    if let Some(ref result) = self.current_probe_result {
                        let content_lines = self.count_probe_popup_lines(result);
                        self.probe_popup_scroll =
                            (self.probe_popup_scroll + 10).min(content_lines.saturating_sub(1));
                    }
                }
                KeyCode::PageUp | KeyCode::Char('b')
                    if key.modifiers == crossterm::event::KeyModifiers::CONTROL =>
                {
                    // Page up
                    self.probe_popup_scroll = self.probe_popup_scroll.saturating_sub(10);
                }
                _ => {} // Ignore other keys
            }
            return Apps::Container { app: self.clone() };
        }

        match key.code {
            Esc => {
                self.key_was_handled_by_container = true;
                // If focused on a detail panel, return to container list
                if self.focused_panel == FocusedPanel::ContainerList {
                    // Navigate back to Pod page
                    debug!("navigating back from container to pod...");
                    let data_vec = vec![];
                    return Apps::Pod {
                        app: crate::tui::pod_app::app::App::new(
                            std::collections::BTreeMap::new(),
                            data_vec,
                        ),
                    };
                }
                self.focused_panel = FocusedPanel::ContainerList;
                self.detail_selection = 0;
            }
            Tab => {
                self.key_was_handled_by_container = true;
                // Cycle through panels: ContainerList -> Mounts -> EnvVars -> Probes -> ContainerList
                self.focused_panel = match self.focused_panel {
                    FocusedPanel::ContainerList => FocusedPanel::Mounts,
                    FocusedPanel::Mounts => FocusedPanel::EnvVars,
                    FocusedPanel::EnvVars => FocusedPanel::Probes,
                    FocusedPanel::Probes => FocusedPanel::ContainerList,
                };
                self.detail_selection = 0;
                self.detail_scroll_offset = 0;
                debug!("Focused panel: {:?}", self.focused_panel);
            }
            BackTab => {
                self.key_was_handled_by_container = true;
                // Cycle backwards through panels
                self.focused_panel = match self.focused_panel {
                    FocusedPanel::ContainerList => FocusedPanel::Probes,
                    FocusedPanel::Mounts => FocusedPanel::ContainerList,
                    FocusedPanel::EnvVars => FocusedPanel::Mounts,
                    FocusedPanel::Probes => FocusedPanel::EnvVars,
                };
                self.detail_selection = 0;
                self.detail_scroll_offset = 0;
                debug!("Focused panel: {:?}", self.focused_panel);
            }
            Up | Down => {
                if self.focused_panel != FocusedPanel::ContainerList {
                    self.key_was_handled_by_container = true;
                    self.handle_detail_navigation(key.code);
                    return Apps::Container { app: self.clone() }; // Handle navigation in focused panel, don't let common handler interfere
                }
                // For container list, let common key handler manage it below
            }
            KeyCode::Char('j') => {
                if self.focused_panel != FocusedPanel::ContainerList {
                    self.key_was_handled_by_container = true;
                    self.handle_detail_navigation(KeyCode::Down);
                    return Apps::Container { app: self.clone() };
                }
                // For container list, let common key handler manage it below
            }
            KeyCode::Char('k') => {
                if self.focused_panel != FocusedPanel::ContainerList {
                    self.key_was_handled_by_container = true;
                    self.handle_detail_navigation(KeyCode::Up);
                    return Apps::Container { app: self.clone() };
                }
                // For container list, let common key handler manage it below
            }
            Enter => {
                match self.focused_panel {
                    FocusedPanel::ContainerList => {
                        // Enter logs for selected container
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
                    }
                    FocusedPanel::Probes => {
                        self.key_was_handled_by_container = true;
                        // Execute the selected probe synchronously and show popup
                        self.execute_selected_probe_sync();
                    }
                    _ => {} // No action for Mounts/EnvVars
                }
            }
            _ => {} // Let other keys be handled by common handler
        }

        Apps::Container { app: self.clone() }
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

    /// Handle navigation within detail panels
    fn handle_detail_navigation(&mut self, key_code: KeyCode) {
        use KeyCode::{Down, Up};

        let max_items = match self.focused_panel {
            FocusedPanel::ContainerList => return, // Handled elsewhere
            FocusedPanel::Mounts => self.get_left_details().len(),
            FocusedPanel::EnvVars => self.get_right_details().len(),
            FocusedPanel::Probes => {
                if let Some(container) = self.get_selected_item() {
                    container.probes.len()
                } else {
                    0
                }
            }
        };

        if max_items == 0 {
            debug!("No items in panel {:?} for navigation", self.focused_panel);
            return;
        }

        let old_selection = self.detail_selection;
        match key_code {
            Down => {
                if self.detail_selection + 1 < max_items {
                    self.detail_selection += 1;
                }
            }
            Up => {
                if self.detail_selection > 0 {
                    self.detail_selection -= 1;
                }
            }
            _ => {}
        }

        // Update scroll offset to keep selected item visible
        // Match the .take(10) limit used in the UI rendering
        const VISIBLE_ITEMS: usize = 10;

        if max_items > VISIBLE_ITEMS {
            if self.detail_selection >= self.detail_scroll_offset + VISIBLE_ITEMS {
                // Scrolling down - selected item is below visible area
                self.detail_scroll_offset = self.detail_selection - VISIBLE_ITEMS + 1;
            } else if self.detail_selection < self.detail_scroll_offset {
                // Scrolling up - selected item is above visible area
                self.detail_scroll_offset = self.detail_selection;
            }
        } else {
            self.detail_scroll_offset = 0;
        }

        debug!(
            "Detail navigation in {:?}: {} -> {} (max: {}, scroll: {}, visible: {})",
            self.focused_panel,
            old_selection,
            self.detail_selection,
            max_items,
            self.detail_scroll_offset,
            VISIBLE_ITEMS
        );

        if old_selection != self.detail_selection {
            debug!(
                "Selection changed from {} to {}",
                old_selection, self.detail_selection
            );
        }
    }

    /// Execute the selected probe and show popup with results
    fn execute_selected_probe_sync(&mut self) {
        let (container, probe_index) = if let Some(container) = self.get_selected_item() {
            (container.clone(), self.detail_selection)
        } else {
            return;
        };

        if probe_index < container.probes.len() {
            let probe = &container.probes[probe_index];
            debug!(
                "Executing probe: {} {}",
                probe.probe_type, probe.handler_type
            );

            // Show a "loading" popup immediately
            self.current_probe_result = Some(ProbeResult {
                probe_type: probe.probe_type.clone(),
                handler_type: probe.handler_type.clone(),
                status: crate::k8s::probes::ProbeStatus::Success, // Will be updated
                response_time_ms: 0,
                status_code: None,
                response_body: "⏳ Executing probe...".to_string(),
                error_message: None,
                timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
            });
            self.show_probe_popup = true;
            self.probe_popup_scroll = 0; // Reset scroll position

            // Execute the probe asynchronously but block for the result
            // This will freeze the UI briefly but provides real probe execution
            let pod_name = container.pod_name.clone();
            let namespace = "default".to_string(); // Use default namespace since that's what the current code assumes
            let probe_clone = probe.clone();

            let result = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    let executor = ProbeExecutor::new(pod_name, namespace);
                    executor.execute_probe(&probe_clone).await
                })
            });

            // Show the result
            self.current_probe_result = Some(result);
        }
    }

    /// Count the number of lines in the probe popup content
    #[allow(clippy::unused_self)] // May use self in future
    fn count_probe_popup_lines(&self, result: &ProbeResult) -> usize {
        let mut lines = 7; // Header, empty line, execution time, timestamp, and possible status code

        if result.status_code.is_some() {
            lines += 1;
        }

        if result.error_message.is_some() {
            lines += 2; // Empty line + error line
        }

        if !result.response_body.is_empty() {
            lines += 2; // Empty line + "Response:" header
            lines += result.response_body.lines().count().min(100); // Limit to 100 lines
        }

        lines
    }

    /// Get probe results for the selected container
    #[allow(clippy::items_after_statements)] // Const is local to this function
    pub fn get_probe_results(&mut self) -> Vec<(String, String, Option<String>)> {
        let container_name = if let Some(container) = self.get_selected_item() {
            container.name.clone()
        } else {
            return vec![];
        };

        if let Some(results) = self.probe_results.get(&container_name) {
            return results
                .iter()
                .map(|result| {
                    let status_text = match result.status {
                        crate::k8s::probes::ProbeStatus::Success => "✓ SUCCESS",
                        crate::k8s::probes::ProbeStatus::Failure => "✗ FAILURE",
                        crate::k8s::probes::ProbeStatus::Timeout => "⏰ TIMEOUT",
                        crate::k8s::probes::ProbeStatus::Error => "❌ ERROR",
                    };

                    let details = if let Some(status_code) = result.status_code {
                        format!(
                            "{} ({}ms) - HTTP {}",
                            status_text, result.response_time_ms, status_code
                        )
                    } else {
                        format!("{} ({}ms)", status_text, result.response_time_ms)
                    };

                    (
                        format!("{} {}", result.probe_type, result.handler_type),
                        details,
                        Some(result.timestamp.clone()),
                    )
                })
                .collect();
        }
        vec![]
    }
}
