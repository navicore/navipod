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
    
    /// Fetch YAML content using kubectl
    /// Fetches YAML content using kubectl
    /// 
    /// # Errors
    /// Returns `io::Error` if kubectl command fails or produces invalid output
    pub fn fetch_yaml(&mut self) -> io::Result<()> {
        self.error_message = None;
        
        let mut cmd = Command::new("kubectl");
        cmd.args(["get", &self.resource_type, &self.resource_name, "-o", "yaml"]);
        
        if let Some(namespace) = &self.namespace {
            cmd.args(["-n", namespace]);
        }
        
        match cmd.output() {
            Ok(output) => {
                if output.status.success() {
                    self.content = String::from_utf8_lossy(&output.stdout).to_string();
                } else {
                    let error = String::from_utf8_lossy(&output.stderr);
                    self.error_message = Some(format!("kubectl error: {error}"));
                    self.content = format!("Error fetching YAML:\n{error}");
                }
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to run kubectl: {e}"));
                self.content = format!("Error: kubectl command failed\n{e}");
            }
        }
        
        Ok(())
    }
    
    
    /// Scroll content up
    pub const fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }
    
    /// Scroll content down
    pub fn scroll_down(&mut self, amount: u16, max_height: u16) {
        let content_lines = u16::try_from(self.content.lines().count()).unwrap_or(u16::MAX);
        let max_scroll = content_lines.saturating_sub(max_height);
        self.scroll_offset = (self.scroll_offset + amount).min(max_scroll);
    }
    
    /// Jump to top of content (vim 'g' motion)
    pub fn jump_to_top(&mut self) {
        self.scroll_offset = 0;
    }
    
    /// Jump to bottom of content (vim 'G' motion)
    pub fn jump_to_bottom(&mut self, max_height: u16) {
        let content_lines = u16::try_from(self.content.lines().count()).unwrap_or(u16::MAX);
        let max_scroll = content_lines.saturating_sub(max_height);
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
        Constraint::Length(3),  // Header
        Constraint::Min(0),     // Content
        Constraint::Length(2),  // Footer
    ]).split(modal_area);
    
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
                .border_style(Style::default().fg(theme.border_focus).bg(theme.bg_secondary))
                .style(Style::default().bg(theme.bg_secondary))
        );
        
    f.render_widget(header, area);
}

fn render_editor_content(f: &mut Frame, editor: &YamlEditor, area: Rect, theme: &NaviTheme) {
    let content_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    
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
        // Simple syntax highlighting for YAML
        let line_style = if line.trim_start().starts_with('#') {
            // Comments
            theme.text_style(TextType::Caption)
        } else if line.trim_end().ends_with(':') && !line.trim_start().starts_with('-') {
            // Keys
            theme.text_style(TextType::Subtitle)
        } else if line.trim_start().starts_with("- ") {
            // List items
            theme.text_style(TextType::Body)
        } else {
            // Values
            theme.text_style(TextType::Body)
        };
        
        // Simple display without line numbers
        let display_line = (*line).to_string();
        
        content_lines.push(Line::styled(display_line, line_style));
    }
    
    let content_paragraph = Paragraph::new(content_lines)
        .style(Style::default().bg(theme.bg_tertiary))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border).bg(theme.bg_tertiary))
                .style(Style::default().bg(theme.bg_tertiary))
        );
        
    f.render_widget(content_paragraph, area);
    
    // Render scrollbar if needed
    if lines.len() > visible_lines {
        let scrollbar_area = area.inner(Margin { vertical: 1, horizontal: 0 });
        let mut scrollbar_state = ratatui::widgets::ScrollbarState::new(lines.len().saturating_sub(visible_lines))
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
                .style(Style::default().bg(theme.bg_secondary))
        );
        
    f.render_widget(footer, area);
}

/// Helper function to create centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ]).split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ]).split(popup_layout[1])[1]
}