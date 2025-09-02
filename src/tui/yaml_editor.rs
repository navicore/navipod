use crate::tui::theme::{NaviTheme, Symbols, TextType};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, Wrap};
use std::io;
use std::process::Command;

/// YAML viewer state (simplified - single read-only mode)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewerMode {
    /// Read-only YAML viewer
    View,
}

/// YAML Viewer state and content
#[derive(Debug, Clone)]
pub struct YamlEditor {
    /// Current mode (always View for simplicity)
    pub mode: ViewerMode,
    /// Raw YAML content to display/edit
    pub content: String,
    /// Kubernetes resource type (for kubectl commands)
    pub resource_type: String,
    /// Resource name
    pub resource_name: String,
    /// Kubernetes namespace (if applicable)
    pub namespace: Option<String>,
    /// Scroll position for view mode
    pub scroll_offset: u16,
    /// Whether editor is currently active/visible
    pub is_active: bool,
    /// Last error message (if any)
    pub error_message: Option<String>,
}

impl Default for YamlEditor {
    fn default() -> Self {
        Self {
            mode: ViewerMode::View,
            content: String::new(),
            resource_type: String::new(),
            resource_name: String::new(),
            namespace: None,
            scroll_offset: 0,
            is_active: false,
            error_message: None,
        }
    }
}

impl YamlEditor {
    /// Create new YAML viewer for a resource
    #[must_use]
    pub fn new(resource_type: String, resource_name: String, namespace: Option<String>) -> Self {
        Self {
            mode: ViewerMode::View,
            resource_type,
            resource_name,
            namespace,
            is_active: true,
            ..Default::default()
        }
    }

    /// Fetches YAML content using kubectl
    ///
    /// # Errors
    /// Returns `io::Error` if kubectl command fails or produces invalid output
    pub fn fetch_yaml(&mut self) -> io::Result<()> {
        self.error_message = None;

        // Validate inputs to prevent command injection
        if !Self::is_safe_kubectl_arg(&self.resource_type) {
            self.error_message =
                Some("Invalid resource type: contains unsafe characters".to_string());
            return Ok(());
        }

        if !Self::is_safe_kubectl_arg(&self.resource_name) {
            self.error_message =
                Some("Invalid resource name: contains unsafe characters".to_string());
            return Ok(());
        }

        if let Some(namespace) = &self.namespace {
            if !Self::is_safe_kubectl_arg(namespace) {
                self.error_message =
                    Some("Invalid namespace: contains unsafe characters".to_string());
                return Ok(());
            }
        }

        let mut cmd = Command::new("kubectl");
        cmd.args([
            "get",
            &self.resource_type,
            &self.resource_name,
            "-o",
            "yaml",
        ]);

        if let Some(namespace) = &self.namespace {
            cmd.args(["-n", namespace]);
        }

        match cmd.output() {
            Ok(output) => {
                if output.status.success() {
                    self.content = String::from_utf8_lossy(&output.stdout).into_owned();
                } else {
                    let error = String::from_utf8_lossy(&output.stderr);
                    self.error_message = Some(format!("kubectl error: {error}"));
                    self.content = format!("Error fetching YAML:\n{error}");
                }
            }
            Err(e) => {
                let cmd_str = format!(
                    "kubectl get {} {} -o yaml",
                    self.resource_type, self.resource_name
                );
                self.error_message =
                    Some(format!("Failed to run kubectl command '{cmd_str}': {e}"));
                self.content = format!("Error: kubectl command failed\n{e}");
            }
        }

        Ok(())
    }

    /// Validates that a string is safe for use as a kubectl argument
    /// Returns false if the string contains potentially unsafe characters
    fn is_safe_kubectl_arg(arg: &str) -> bool {
        // Allow alphanumeric characters, hyphens, underscores, dots, and colons
        // This covers most valid Kubernetes resource names and namespaces
        arg.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
            && !arg.is_empty()
            && !arg.starts_with('-') // Avoid flag injection
    }

    /// Scroll content up
    pub const fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    /// Scroll content down with dynamic height calculation
    pub fn scroll_down(&mut self, amount: u16, viewport_height: Option<u16>) {
        let content_lines = u16::try_from(self.content.lines().count()).unwrap_or(u16::MAX);
        // Use provided viewport height or default to reasonable content area size
        let effective_height = viewport_height.unwrap_or_else(|| {
            // Calculate a reasonable default based on typical terminal size
            // Subtract space for header (3), footer (2), borders, and margins
            std::cmp::max(10, 24_u16.saturating_sub(7))
        });
        let max_scroll = content_lines.saturating_sub(effective_height);
        self.scroll_offset = (self.scroll_offset + amount).min(max_scroll);
    }

    /// Jump to top of content (vim 'g' motion)
    pub const fn jump_to_top(&mut self) {
        self.scroll_offset = 0;
    }

    /// Jump to bottom of content (vim 'G' motion)
    pub fn jump_to_bottom(&mut self, viewport_height: Option<u16>) {
        let content_lines = u16::try_from(self.content.lines().count()).unwrap_or(u16::MAX);
        // Use provided viewport height or default to reasonable content area size
        let effective_height = viewport_height.unwrap_or_else(|| {
            // Calculate a reasonable default based on typical terminal size
            // Subtract space for header (3), footer (2), borders, and margins
            std::cmp::max(10, 24_u16.saturating_sub(7))
        });
        let max_scroll = content_lines.saturating_sub(effective_height);
        self.scroll_offset = max_scroll;
    }

    /// Close the editor
    pub fn close(&mut self) {
        self.is_active = false;
        self.content.clear();
        self.error_message = None;
        self.scroll_offset = 0;
    }
}

/// Render the floating YAML editor overlay
pub fn render_yaml_editor(f: &mut Frame, editor: &YamlEditor) {
    if !editor.is_active {
        return;
    }

    let theme = NaviTheme::default();
    let area = f.area();

    // Create floating window (80% of screen)
    let modal_area = centered_rect(80, 80, area);

    // Clear background
    f.render_widget(Clear, modal_area);

    // Main editor layout
    let editor_chunks = Layout::vertical([
        Constraint::Length(3), // Header
        Constraint::Min(0),    // Content
        Constraint::Length(2), // Footer
    ])
    .split(modal_area);

    // Render header
    render_editor_header(f, editor, editor_chunks[0], &theme);

    // Render content
    render_editor_content(f, editor, editor_chunks[1], &theme);

    // Render footer
    render_editor_footer(f, editor, editor_chunks[2], &theme);
}

fn render_editor_header(f: &mut Frame, editor: &YamlEditor, area: Rect, theme: &NaviTheme) {
    let title = format!(
        "{} YAML Viewer - {} {}",
        Symbols::CHEVRON_RIGHT,
        editor.resource_type,
        editor.resource_name
    );

    let header_style = theme.text_style(TextType::Title);

    let header = Paragraph::new(title)
        .style(header_style.bg(theme.bg_secondary))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(
                    Style::default()
                        .fg(theme.border_focus)
                        .bg(theme.bg_secondary),
                )
                .style(Style::default().bg(theme.bg_secondary)),
        );

    f.render_widget(header, area);
}

fn render_editor_content(f: &mut Frame, editor: &YamlEditor, area: Rect, theme: &NaviTheme) {
    let content_area = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });

    // Handle error state
    if let Some(error) = &editor.error_message {
        let error_text = format!("Error: {error}\n\nPress 'q' to close, 'r' to retry");
        let error_paragraph = Paragraph::new(error_text)
            .style(theme.text_style(TextType::Error).bg(theme.bg_tertiary))
            .wrap(Wrap { trim: true });
        f.render_widget(error_paragraph, content_area);
        return;
    }

    // Render YAML content with syntax highlighting hints
    let lines: Vec<&str> = editor.content.lines().collect();
    let visible_lines = content_area.height as usize;
    let start_line = editor.scroll_offset as usize;
    let end_line = (start_line + visible_lines).min(lines.len());

    let mut content_lines = Vec::new();
    for line in &lines[start_line..end_line] {
        // Parse line into styled segments for proper YAML highlighting
        let styled_line = parse_yaml_line(line, theme);
        content_lines.push(styled_line);
    }

    let content_paragraph = Paragraph::new(content_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border).bg(theme.bg_tertiary))
            .style(Style::default().bg(theme.bg_tertiary)),
    );

    f.render_widget(content_paragraph, area);

    // Render scrollbar if needed
    if lines.len() > visible_lines {
        let scrollbar_area = area.inner(Margin {
            vertical: 1,
            horizontal: 0,
        });
        let mut scrollbar_state =
            ratatui::widgets::ScrollbarState::new(lines.len().saturating_sub(visible_lines))
                .position(start_line);

        f.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(theme.border).bg(theme.bg_tertiary))
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"))
                .track_symbol(Some("│"))
                .thumb_symbol("█"),
            scrollbar_area,
            &mut scrollbar_state,
        );
    }
}

fn render_editor_footer(f: &mut Frame, _editor: &YamlEditor, area: Rect, theme: &NaviTheme) {
    let help_text = "q: Close • j/k/↑↓: Scroll • G: Bottom • g: Top • r: Refresh";

    let footer = Paragraph::new(help_text)
        .style(theme.text_style(TextType::Caption).bg(theme.bg_secondary))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.divider).bg(theme.bg_secondary))
                .style(Style::default().bg(theme.bg_secondary)),
        );

    f.render_widget(footer, area);
}

/// Parse a YAML line into styled segments for proper syntax highlighting
///
/// Color scheme (muted):
/// - Keys: Subtitle (blue, no bold) - e.g., `name:`  
/// - List markers: Caption (muted) - e.g., `- `
/// - Quoted strings: Warning (yellow, no bold) - e.g., `"value"`
/// - Unquoted values: Body (default text) - e.g., `my-value`
/// - Numbers: Subtitle (blue) - e.g., `123`
/// - Booleans: Warning (yellow) - e.g., `true`, `false`
/// - Null: Caption (muted) - e.g., `null`
/// - Comments: Caption (muted) - e.g., `# comment`
/// - YAML separators: Caption (muted) - e.g., `---`
fn parse_yaml_line(line: &str, theme: &NaviTheme) -> Line<'static> {
    let mut spans = Vec::new();

    // Handle leading whitespace (indentation)
    let leading_spaces = line.len() - line.trim_start().len();
    if leading_spaces > 0 {
        spans.push(Span::styled(
            " ".repeat(leading_spaces),
            Style::default().bg(theme.bg_tertiary),
        ));
    }

    let trimmed = line.trim_start();

    // Handle empty lines
    if trimmed.is_empty() {
        spans.push(Span::styled("", Style::default().bg(theme.bg_tertiary)));
        return Line::from(spans);
    }

    // Handle full-line comments
    if trimmed.starts_with('#') {
        spans.push(Span::styled(
            trimmed.to_string(),
            theme.text_style(TextType::Caption).bg(theme.bg_tertiary),
        ));
        return Line::from(spans);
    }

    // Handle document separators
    if trimmed == "---" || trimmed == "..." {
        spans.push(Span::styled(
            trimmed.to_string(),
            theme.text_style(TextType::Caption).bg(theme.bg_tertiary),
        ));
        return Line::from(spans);
    }

    // Handle list items
    if let Some(rest) = trimmed.strip_prefix("- ") {
        spans.push(Span::styled(
            "- ",
            theme.text_style(TextType::Caption).bg(theme.bg_tertiary),
        ));

        // Parse the rest of the line after the list marker
        spans.extend(parse_yaml_value(rest, theme));
        return Line::from(spans);
    }

    // Handle key-value pairs
    if let Some(colon_pos) = trimmed.find(':') {
        let key = &trimmed[..colon_pos];
        let rest = &trimmed[colon_pos..];

        // Key part (muted blue)
        spans.push(Span::styled(
            key.to_string(),
            Style::default()
                .fg(theme.text_secondary)
                .bg(theme.bg_tertiary), // Subtitle without bold
        ));

        // Colon
        spans.push(Span::styled(
            ":",
            Style::default()
                .fg(theme.text_secondary)
                .bg(theme.bg_tertiary),
        ));

        // Value part (after colon)
        if rest.len() > 1 {
            let value_part = &rest[1..]; // Skip the colon
            spans.extend(parse_yaml_value(value_part, theme));
        }

        return Line::from(spans);
    }

    // Default: treat as value
    spans.extend(parse_yaml_value(trimmed, theme));
    Line::from(spans)
}

/// Parse a YAML value part, handling comments, strings, and special values
fn parse_yaml_value(value: &str, theme: &NaviTheme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    // Check for inline comments
    if let Some(comment_pos) = value.find(" #") {
        let value_part = &value[..comment_pos];
        let comment_part = &value[comment_pos..];

        // Add value part
        if !value_part.trim().is_empty() {
            spans.extend(parse_value_tokens(value_part, theme));
        }

        // Add comment part
        spans.push(Span::styled(
            comment_part.to_string(),
            theme.text_style(TextType::Caption).bg(theme.bg_tertiary),
        ));
    } else {
        // No comment, parse the whole value
        spans.extend(parse_value_tokens(value, theme));
    }

    spans
}

/// Parse value tokens (strings, numbers, booleans, null)
fn parse_value_tokens(value: &str, theme: &NaviTheme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let trimmed = value.trim();

    if trimmed.is_empty() {
        spans.push(Span::styled(
            value.to_string(),
            Style::default().bg(theme.bg_tertiary),
        ));
        return spans;
    }

    // Handle special YAML values - distinct but muted colors
    let style = match trimmed {
        "null" | "~" => theme.text_style(TextType::Caption).bg(theme.bg_tertiary),
        "true" | "false" => {
            // Booleans - soft orange/amber without bold
            Style::default()
                .fg(Color::from_u32(0x00D9_7706))
                .bg(theme.bg_tertiary)
        }
        _ if trimmed.starts_with('"') && trimmed.ends_with('"') => {
            // Quoted strings - soft yellow without bold
            Style::default()
                .fg(Color::from_u32(0x00CA_8A04))
                .bg(theme.bg_tertiary)
        }
        _ if trimmed.starts_with('\'') && trimmed.ends_with('\'') => {
            // Single quoted strings - soft yellow without bold
            Style::default()
                .fg(Color::from_u32(0x00CA_8A04))
                .bg(theme.bg_tertiary)
        }
        _ if trimmed
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '-') =>
        {
            // Numbers - soft blue without bold
            Style::default()
                .fg(Color::from_u32(0x003B_82F6))
                .bg(theme.bg_tertiary)
        }
        _ => {
            // Regular unquoted values - blood red for distinction
            Style::default()
                .fg(Color::from_u32(0x00DC_2626))
                .bg(theme.bg_tertiary)
        }
    };

    spans.push(Span::styled(value.to_string(), style));
    spans
}

/// Helper function to create centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}
