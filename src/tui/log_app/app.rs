use crate::impl_tui_table_state;
use crate::k8s::containers::logs_enhanced;
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
use tracing::{debug, warn};

const POLL_MS: u64 = 5000;

#[derive(Clone, Debug)]
pub struct App {
    pub(crate) base: BaseTableState<LogRec>,
    pub(crate) selector: BTreeMap<String, String>,
    pub(crate) pod_name: String,
    pub(crate) container_name: String,
    /// Whether auto-tailing is enabled (follows new logs)
    pub(crate) is_tailing: bool,
    /// Track if user manually scrolled (to pause auto-tailing)
    pub(crate) user_scrolled: bool,
    /// Maximum number of log lines to keep in memory
    pub(crate) max_log_lines: usize,
}

impl_tui_table_state!(App, LogRec);

impl AppBehavior for log_app::app::App {
    async fn handle_event(&mut self, event: &Message) -> Result<Option<Apps>, io::Error> {
        if self.get_show_filter_edit() {
            Ok(Some(self.handle_filter_edit_event(event)))
        } else {
            Ok(self.handle_table_event(event))
        }
    }

    fn draw_ui<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), std::io::Error> {
        terminal.draw(|f| super::modern_ui::ui(f, self))?; // Use modern UI
        Ok(())
    }

    fn stream(&self, should_stop: Arc<AtomicBool>) -> impl Stream<Item = Message> {
        let (tx, rx) = mpsc::channel(1000); // Larger buffer for streaming logs

        let pod_name = self.pod_name.clone();
        let container_name = self.container_name.clone();
        let selector = self.selector.clone();

        tokio::spawn(async move {
            debug!("Starting enhanced log stream for pod: {}, container: {}", pod_name, container_name);
            
            // First, get initial logs (last 100 lines) without following
            match logs_enhanced(
                selector.clone(),
                pod_name.clone(),
                container_name.clone(),
                false, // don't follow for initial logs
                Some(100),
            ).await {
                Ok(initial_logs) => {
                    if !initial_logs.is_empty() && tx.send(Message::Log(initial_logs)).await.is_err() {
                        debug!("Failed to send initial logs, receiver dropped");
                        return;
                    }
                }
                Err(e) => {
                    warn!("Failed to get initial logs: {}", e);
                }
            }
            
            // Now start polling for new logs with follow=true for real-time updates
            // Use a shorter interval for more responsive streaming
            const STREAM_POLL_MS: u64 = 1000; // 1 second for responsive streaming
            let mut last_log_count = 0;
            
            while !should_stop.load(Ordering::Relaxed) {
                match logs_enhanced(
                    selector.clone(),
                    pod_name.clone(),
                    container_name.clone(),
                    true, // follow for real-time logs
                    Some(200), // Get more lines to catch up
                ).await {
                    Ok(all_logs) => {
                        let current_log_count = all_logs.len();
                        
                        // Only send new logs if we have more than before
                        if current_log_count > last_log_count {
                            let new_logs: Vec<LogRec> = if last_log_count == 0 {
                                // First time, send last 50 logs to avoid flooding
                                all_logs.into_iter().skip(current_log_count.saturating_sub(50)).collect()
                            } else {
                                // Get only the new logs since last fetch
                                all_logs.into_iter().skip(last_log_count).collect()
                            };
                            
                            if !new_logs.is_empty() {
                                last_log_count = current_log_count;
                                if tx.send(Message::Log(new_logs)).await.is_err() {
                                    debug!("Failed to send log batch, receiver dropped");
                                    break;
                                }
                            }
                        } else {
                            // Reset counter if log count decreased (log rotation)
                            if current_log_count < last_log_count {
                                last_log_count = current_log_count;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Error fetching streaming logs: {}", e);
                        // Continue trying after a longer delay on error
                        sleep(Duration::from_millis(POLL_MS)).await;
                        continue;
                    }
                }
                
                sleep(Duration::from_millis(STREAM_POLL_MS)).await;
            }
            
            debug!("Log stream ended for pod: {}, container: {}", pod_name, container_name);
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
            is_tailing: true,  // Start with tailing enabled
            user_scrolled: false,
            max_log_lines: 1000, // Keep last 1000 lines in memory
        }
    }

    /// Check if we're at the bottom of the log (should auto-tail)
    pub fn is_at_bottom(&self) -> bool {
        if self.base.items.is_empty() {
            return true;
        }
        
        // Check if current selection is at or near the end
        let current_selection = self.base.state.selected().unwrap_or(0);
        let total_items = self.base.items.len();
        
        // Consider "at bottom" if within 3 lines of the end
        total_items.saturating_sub(current_selection) <= 3
    }

    /// Enable/disable auto-tailing
    pub fn set_tailing(&mut self, enabled: bool) {
        self.is_tailing = enabled;
        if enabled {
            // Jump to bottom when re-enabling tailing
            self.jump_to_bottom();
            self.user_scrolled = false;
        }
    }

    /// Jump to the bottom of the logs
    pub fn jump_to_bottom(&mut self) {
        if !self.base.items.is_empty() {
            let last_index = self.base.items.len() - 1;
            self.base.state.select(Some(last_index));
            // Update scroll state to show bottom
            self.base.scroll_state = ratatui::widgets::ScrollbarState::new(
                self.base.items.len().saturating_sub(1) * ITEM_HEIGHT
            ).position(last_index * ITEM_HEIGHT);
        }
    }

    /// Add new log entries and maintain buffer size
    pub fn add_log_entries(&mut self, mut new_logs: Vec<LogRec>) {
        let was_at_bottom = self.is_at_bottom();
        
        // Add new logs to the end
        self.base.items.append(&mut new_logs);
        
        // Trim old logs if we exceed the maximum
        if self.base.items.len() > self.max_log_lines {
            let excess = self.base.items.len() - self.max_log_lines;
            self.base.items.drain(0..excess);
            
            // Adjust selection if needed
            if let Some(current) = self.base.state.selected() {
                if current >= excess {
                    self.base.state.select(Some(current - excess));
                } else {
                    self.base.state.select(Some(0));
                }
            }
        }
        
        // Update scroll state
        self.base.scroll_state = ratatui::widgets::ScrollbarState::new(
            self.base.items.len().saturating_sub(1) * ITEM_HEIGHT
        );
        
        // Auto-tail if enabled and we were at bottom
        if self.is_tailing && (was_at_bottom || !self.user_scrolled) {
            self.jump_to_bottom();
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
            Char('t' | 'T') => {
                // Toggle auto-tailing
                self.set_tailing(!self.is_tailing);
                debug!("Auto-tailing {}", if self.is_tailing { "enabled" } else { "disabled" });
                Apps::Log { app: self.clone() }
            }
            Char('G') => {
                // Jump to bottom (vim-style)
                self.jump_to_bottom();
                self.set_tailing(true); // Re-enable tailing when jumping to bottom
                debug!("Jumped to bottom, auto-tailing enabled");
                Apps::Log { app: self.clone() }
            }
            Char('g') => {
                // Jump to top (vim-style) - this will disable tailing
                if !self.base.items.is_empty() {
                    self.base.state.select(Some(0));
                    self.base.scroll_state = ratatui::widgets::ScrollbarState::new(
                        self.base.items.len().saturating_sub(1) * ITEM_HEIGHT
                    ).position(0);
                    self.set_tailing(false); // Disable tailing when manually navigating
                    self.user_scrolled = true;
                    debug!("Jumped to top, auto-tailing disabled");
                }
                Apps::Log { app: self.clone() }
            }
            Enter => {
                // noop for now but will be pretty printed detail analysis popup
                Apps::Log { app: self.clone() }
            }
            _ => Apps::Log { app: self.clone() },
        }
    }

    /// Track navigation keys to detect user scrolling
    pub fn handle_navigation_key(&mut self, key: &crossterm::event::KeyEvent) -> bool {
        use KeyCode::{Up, Down, PageUp, PageDown, Home, End, Char};
        
        let was_at_bottom = self.is_at_bottom();
        
        match key.code {
            Up | Char('k' | 'j') | Down | PageUp | PageDown | Home | End => {
                // Mark that user has manually scrolled
                self.user_scrolled = true;
                
                // If user scrolls away from bottom, disable auto-tailing
                if !was_at_bottom || !self.is_at_bottom() {
                    self.set_tailing(false);
                }
                true // Handled
            }
            _ => false // Not a navigation key
        }
    }

    fn handle_table_event(&mut self, event: &Message) -> Option<Apps> {
        match event {
            Message::Key(Event::Key(key)) => {
                if key.kind == KeyEventKind::Press {
                    // Handle ESC specially to return None for history navigation
                    if key.code == KeyCode::Esc {
                        debug!("navigating back from log to container...");
                        return None; // This will use the history stack
                    }
                    
                    // Check for navigation keys first to track user scrolling
                    if self.handle_navigation_key(key) {
                        // Navigation was handled, still need to process through common keys
                    }
                    
                    // First try common keys (navigation, quit, color, vim motions)
                    return match handle_common_keys(self, key, |app| Apps::Log { app }) {
                        KeyHandlerResult::Quit => None,
                        KeyHandlerResult::HandledWithUpdate(app_holder) | KeyHandlerResult::Handled(app_holder) => app_holder,
                        KeyHandlerResult::NotHandled => {
                            // Handle Log-specific keys
                            Some(self.handle_log_specific_keys(key))
                        }
                    };
                }
                Some(Apps::Log { app: self.clone() })
            }
            Message::Log(data_vec) => {
                // Use the new smart tailing logic instead of replacing all data
                let mut new_app = self.clone();
                if data_vec != &new_app.base.items {
                    // Convert to new logs only (this is a simplification - in real streaming we'd track what's new)
                    new_app.add_log_entries(data_vec.clone());
                }
                Some(Apps::Log { app: new_app })
            }
            _ => Some(Apps::Log { app: self.clone() })
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
