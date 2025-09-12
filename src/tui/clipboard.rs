use arboard::Clipboard;
use tracing::{debug, warn};

/// Clipboard operations for TUI application
pub struct ClipboardManager {
    clipboard: Result<Clipboard, arboard::Error>,
}

impl ClipboardManager {
    pub fn new() -> Self {
        Self {
            clipboard: Clipboard::new(),
        }
    }

    /// Copy text content to clipboard
    pub fn copy_text(&mut self, content: &str) -> Result<(), String> {
        match &mut self.clipboard {
            Ok(clipboard) => {
                match clipboard.set_text(content) {
                    Ok(()) => {
                        debug!("Successfully copied {} characters to clipboard", content.len());
                        Ok(())
                    }
                    Err(e) => {
                        warn!("Failed to copy to clipboard: {}", e);
                        Err(format!("Copy failed: {}", e))
                    }
                }
            }
            Err(e) => {
                warn!("Clipboard not available: {}", e);
                Err(format!("Clipboard unavailable: {}", e))
            }
        }
    }

    /// Copy log lines with optional formatting
    pub fn copy_logs(&mut self, logs: &[String], include_timestamps: bool) -> Result<(), String> {
        if logs.is_empty() {
            return Err("No logs to copy".to_string());
        }

        let content = if include_timestamps {
            logs.join("\n")
        } else {
            // Strip timestamps if present (assume format: "timestamp message")
            logs.iter()
                .map(|line| {
                    // Try to find the first space after a timestamp pattern
                    if let Some(first_space) = line.find(' ') {
                        // Check if it looks like a timestamp (contains colons and possibly brackets)
                        let potential_timestamp = &line[..first_space];
                        if potential_timestamp.contains(':') || potential_timestamp.contains('[') {
                            &line[first_space + 1..]
                        } else {
                            line
                        }
                    } else {
                        line
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        self.copy_text(&content)
    }

    /// Copy probe result in a formatted way
    pub fn copy_probe_result(&mut self, probe_result: &crate::k8s::probes::ProbeResult) -> Result<(), String> {
        let formatted = format!(
            "Probe Result - {} {}\n\
            Status: {:?}\n\
            Response Time: {}ms\n\
            Timestamp: {}\n\
            {}\n\
            {}",
            probe_result.probe_type,
            probe_result.handler_type,
            probe_result.status,
            probe_result.response_time_ms,
            probe_result.timestamp,
            probe_result.status_code
                .map(|code| format!("HTTP Status: {}", code))
                .unwrap_or_default(),
            if probe_result.response_body.is_empty() {
                "No response body".to_string()
            } else {
                format!("Response:\n{}", probe_result.response_body)
            }
        );

        self.copy_text(&formatted)
    }

    /// Copy probe configuration details
    pub fn copy_probe_config(&mut self, probe: &crate::tui::data::ContainerProbe) -> Result<(), String> {
        let formatted = format!(
            "Probe Configuration\n\
            Type: {}\n\
            Handler: {}\n\
            Details: {}",
            probe.probe_type,
            probe.handler_type,
            probe.details
        );

        self.copy_text(&formatted)
    }

    /// Copy container environment variables
    pub fn copy_env_vars(&mut self, env_vars: &[(String, String, Option<String>)]) -> Result<(), String> {
        if env_vars.is_empty() {
            return Err("No environment variables to copy".to_string());
        }

        let formatted = env_vars
            .iter()
            .map(|(key, value, _)| format!("{}={}", key, value))
            .collect::<Vec<_>>()
            .join("\n");

        self.copy_text(&formatted)
    }

    /// Copy container mount information
    pub fn copy_mounts(&mut self, mounts: &[(String, String, Option<String>)]) -> Result<(), String> {
        if mounts.is_empty() {
            return Err("No mounts to copy".to_string());
        }

        let formatted = mounts
            .iter()
            .map(|(name, path, _)| format!("{}: {}", name, path))
            .collect::<Vec<_>>()
            .join("\n");

        self.copy_text(&formatted)
    }
}

impl Default for ClipboardManager {
    fn default() -> Self {
        Self::new()
    }
}