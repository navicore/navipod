use crate::tui::log_app::app::App;
use crate::tui::table_ui::TuiTableState;
use crate::tui::theme::{NaviTheme, Symbols, TextType, UiConstants, UiHelpers};
use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Clear, Paragraph, Scrollbar, 
    ScrollbarOrientation, Wrap
};

const LOG_HEIGHT: u16 = 2; // 2 lines per log entry for readability

/// Modern streaming log viewer UI with syntax highlighting and log-level awareness
pub fn ui(f: &mut Frame, app: &App) {
    let theme = NaviTheme::default();
    
    // Set the main background to ensure consistent theming
    let main_bg = Block::default()
        .style(Style::default().bg(theme.bg_primary));
    f.render_widget(main_bg, f.area());
    
    // Main layout: header, content, footer
    let main_chunks = Layout::vertical([
        Constraint::Length(UiConstants::HEADER_HEIGHT),  // Header
        Constraint::Min(0),     // Content (flexible)
        Constraint::Length(UiConstants::FOOTER_HEIGHT),  // Footer
    ]).split(f.area());
    
    render_header(f, app, main_chunks[0], &theme);
    render_content(f, app, main_chunks[1], &theme);
    render_footer(f, main_chunks[2], &theme);
    
    // Handle overlays
    if app.get_show_filter_edit() {
        render_filter_modal(f, app, &theme);
    }
}

fn render_header(f: &mut Frame, app: &App, area: Rect, theme: &NaviTheme) {
    let header_chunks = Layout::horizontal([
        Constraint::Length(UiConstants::ACTIONS_COLUMN_WIDTH),  // Icon + Title + Container info
        Constraint::Min(0),      // Context info (flexible)
        Constraint::Length(UiConstants::DETAILS_PANEL_WIDTH),  // Actions
    ]).split(area);
    
    // Title with container info
    let title_text = format!("{} Logs • {}/{}", 
        Symbols::CONTAINER, 
        truncate_text(&app.pod_name, 10),
        truncate_text(&app.container_name, 8)
    );
    let title = Paragraph::new(title_text)
        .style(theme.text_style(TextType::Title).bg(theme.bg_primary))
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(title, header_chunks[0]);
    
    // Context info (log stats and streaming status)
    let logs = app.get_items();
    let total_count = logs.len();
    
    // Count log levels
    let error_count = logs.iter().filter(|l| l.level.to_lowercase().contains("error") || l.level.to_lowercase().contains("err")).count();
    let warn_count = logs.iter().filter(|l| l.level.to_lowercase().contains("warn")).count();
    
    let context_text = if error_count > 0 {
        format!("{total_count} logs • {error_count} errors • {warn_count} warnings • LIVE")
    } else if warn_count > 0 {
        format!("{total_count} logs • {warn_count} warnings • LIVE")
    } else {
        format!("{total_count} logs • LIVE")
    };
    
    let context = Paragraph::new(context_text)
        .style(theme.text_style(TextType::Caption).bg(theme.bg_primary))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(context, header_chunks[1]);
    
    // Actions/shortcuts
    let actions_text = "/: filter • Enter: details • c: colors • q: quit";
    let actions = Paragraph::new(actions_text)
        .style(theme.text_style(TextType::Caption).bg(theme.bg_primary))
        .alignment(Alignment::Right)
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(actions, header_chunks[2]);
    
    // Divider line
    let divider = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme.divider).bg(theme.bg_primary))
        .style(Style::default().bg(theme.bg_primary));
    f.render_widget(divider, area);
}

fn render_content(f: &mut Frame, app: &App, area: Rect, theme: &NaviTheme) {
    render_log_stream(f, app, area, theme);
}

fn render_log_stream(f: &mut Frame, app: &App, area: Rect, theme: &NaviTheme) {
    let items = app.get_filtered_items();
    let selected_index = app.state.selected().unwrap_or(0);
    
    let content_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    
    let filter = app.get_filter();
    let title = if filter.is_empty() {
        "Log Stream".to_string()
    } else {
        format!("Log Stream (filtered: {filter})")
    };
    
    // Render container block
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border).bg(theme.bg_secondary))
        .title_style(theme.text_style(TextType::Subtitle).bg(theme.bg_secondary))
        .style(Style::default().bg(theme.bg_secondary));
    f.render_widget(block, area);
    
    // Calculate scroll offset to keep selected item visible
    // For logs, we want to show latest entries at the bottom (typical log viewer behavior)
    let visible_logs = content_area.height / LOG_HEIGHT;
    
    // Auto-scroll to bottom for new logs unless user is actively browsing
    let scroll_offset = if selected_index >= items.len().saturating_sub(visible_logs as usize) {
        // User is near the bottom or at the end, show latest logs
        items.len().saturating_sub(visible_logs as usize)
    } else {
        // User is browsing older logs, maintain their position
        if UiHelpers::safe_cast_u16(selected_index, "log scroll offset") >= visible_logs {
            selected_index.saturating_sub(visible_logs as usize / 2)
        } else {
            0
        }
    };
    
    // Render individual log entries with scroll offset
    let mut y_offset = 0;
    for (index, log) in items.iter().enumerate().skip(scroll_offset) {
        if y_offset + LOG_HEIGHT > content_area.height {
            break; // Don't render beyond visible area
        }
        
        let is_selected = index == selected_index;
        let log_area = Rect {
            x: content_area.x,
            y: content_area.y + y_offset,
            width: content_area.width,
            height: LOG_HEIGHT.min(content_area.height - y_offset),
        };
        
        render_log_entry(f, log, log_area, is_selected, theme);
        y_offset += LOG_HEIGHT;
    }
    
    // Render scrollbar
    render_log_scrollbar(f, app, area, theme);
}

fn render_log_entry(f: &mut Frame, log: &crate::tui::data::LogRec, area: Rect, is_selected: bool, theme: &NaviTheme) {
    // Parse log level and determine styling
    let level_style = get_log_level_style(&log.level, theme);
    let level_symbol = get_log_level_symbol(&log.level);
    
    // Extract time part from datetime (assume format like "2024-01-01 12:34:56")
    let time_part = extract_time(&log.datetime);
    
    // Card background - ensure proper contrast
    let log_bg = if is_selected { theme.bg_accent } else { theme.bg_tertiary };
    let selection_indicator = if is_selected { "▶ " } else { "  " };
    
    // Create log entry content as multi-line text
    let content = vec![
        Line::from(vec![
            Span::raw(selection_indicator),
            Span::styled(level_symbol, level_style),
            Span::raw(" "),
            Span::styled(time_part, theme.text_style(TextType::Caption)),
            Span::raw(" "),
            Span::styled(format!("[{}]", log.level.to_uppercase()), level_style),
        ]),
        Line::from(vec![
            Span::raw("    "),
            Span::styled(truncate_text(&log.message, (area.width.saturating_sub(6)) as usize), 
                        theme.text_style(TextType::Body)),
        ]),
    ];
    
    let log_entry = Paragraph::new(content)
        .style(Style::default().bg(log_bg))
        .wrap(Wrap { trim: false });
    
    f.render_widget(log_entry, area);
}

fn render_log_scrollbar(f: &mut Frame, app: &App, area: Rect, theme: &NaviTheme) {
    let items = app.get_filtered_items();
    let content_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    let visible_logs = content_area.height / LOG_HEIGHT;
    
    // Show scrollbar if we have more items than can fit
    if items.len() > visible_logs as usize {
        let selected_index = app.state.selected().unwrap_or(0);
        
        // Calculate scrollbar position based on selection
        let mut scrollbar_state = ratatui::widgets::ScrollbarState::new(items.len().saturating_sub(visible_logs as usize))
            .position(selected_index.saturating_sub(visible_logs as usize / 2));
        
        f.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(theme.border).bg(theme.bg_secondary))
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"))
                .track_symbol(Some("│"))
                .thumb_symbol("█"),
            area.inner(Margin { vertical: 1, horizontal: 0 }),
            &mut scrollbar_state,
        );
    }
}

fn render_footer(f: &mut Frame, area: Rect, theme: &NaviTheme) {
    let footer_text = "↑↓: Navigate • /: Filter • End: Jump to latest • Home: Jump to oldest • Ctrl+F/B: Page Up/Down";
    let footer = Paragraph::new(footer_text)
        .style(theme.text_style(TextType::Caption).bg(theme.bg_primary))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(theme.divider).bg(theme.bg_primary))
                .style(Style::default().bg(theme.bg_primary))
        );
    
    f.render_widget(footer, area);
}

fn render_filter_modal(f: &mut Frame, app: &App, theme: &NaviTheme) {
    let area = f.area();
    let modal_area = centered_rect(70, 25, area);
    
    // Clear background
    f.render_widget(Clear, modal_area);
    
    // Modal content
    let filter_text = if app.get_filter().is_empty() {
        "Enter regex filter pattern...".to_string()
    } else {
        app.get_filter()
    };
    
    let filter_input = Paragraph::new(filter_text)
        .style(if app.get_filter().is_empty() {
            theme.text_style(TextType::Caption).bg(theme.bg_secondary)
        } else {
            theme.text_style(TextType::Body).bg(theme.bg_secondary)
        })
        .block(
            Block::default()
                .title(format!("{} Filter Log Messages", Symbols::CHEVRON_RIGHT))
                .title_style(theme.text_style(TextType::Subtitle).bg(theme.bg_secondary))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_focus).bg(theme.bg_secondary).add_modifier(Modifier::BOLD))
                .style(Style::default().bg(theme.bg_secondary))
        )
        .wrap(Wrap { trim: true });
    
    f.render_widget(filter_input, modal_area);
    
    // Set cursor position
    let cursor_pos = Position {
        x: modal_area.x + UiHelpers::safe_cast_u16(app.get_cursor_pos(), "log cursor position") + 1,
        y: modal_area.y + 1,
    };
    f.set_cursor_position(cursor_pos);
    
    // Help text
    let help_area = Rect {
        x: modal_area.x,
        y: modal_area.y + modal_area.height,
        width: modal_area.width,
        height: 2,
    };
    
    let help_text = vec![
        Line::from("ESC: Cancel • Enter: Apply • Examples: '(error|warn)', 'failed.*request', '^INFO'"),
        Line::from("Filter searches in log messages using regex patterns • Case insensitive"),
    ];
    
    let help = Paragraph::new(help_text)
        .style(theme.text_style(TextType::Caption).bg(theme.bg_primary))
        .alignment(Alignment::Center)
        .block(Block::default().style(Style::default().bg(theme.bg_primary)));
    
    f.render_widget(help, help_area);
}

// Helper functions

/// Get log level symbol based on level string
fn get_log_level_symbol(level: &str) -> &'static str {
    match level.to_lowercase().as_str() {
        s if s.contains("error") || s.contains("err") || s.contains("fatal") => Symbols::ERROR,
        s if s.contains("warn") || s.contains("warning") => Symbols::WARNING,
        s if s.contains("info") || s.contains("information") => Symbols::SUCCESS,
        s if s.contains("debug") || s.contains("trace") => Symbols::DOT,
        _ => Symbols::BULLET,
    }
}

/// Get appropriate text style for log level
fn get_log_level_style(level: &str, theme: &NaviTheme) -> Style {
    match level.to_lowercase().as_str() {
        s if s.contains("error") || s.contains("err") || s.contains("fatal") => theme.text_style(TextType::Error),
        s if s.contains("warn") || s.contains("warning") => theme.text_style(TextType::Warning),
        s if s.contains("info") || s.contains("information") => theme.text_style(TextType::Success),
        s if s.contains("debug") || s.contains("trace") => theme.text_style(TextType::Caption),
        _ => theme.text_style(TextType::Body),
    }
}

/// Extract time portion from datetime string
#[allow(clippy::option_if_let_else)] // Complex nested logic is more readable with if/let
fn extract_time(datetime: &str) -> String {
    // Handle various datetime formats
    if let Some(space_pos) = datetime.find(' ') {
        // Format: "2024-01-01 12:34:56" -> "12:34:56"
        let time_part = &datetime[space_pos + 1..];
        if time_part.len() >= 8 {
            // Take first 8 characters (HH:MM:SS)
            time_part[..8].to_string()
        } else {
            time_part.to_string()
        }
    } else if datetime.contains('T') {
        // ISO format: "2024-01-01T12:34:56Z" -> "12:34:56"
        if let Some(t_pos) = datetime.find('T') {
            let time_part = &datetime[t_pos + 1..];
            if let Some(z_pos) = time_part.find('Z') {
                time_part[..z_pos.min(8)].to_string()
            } else {
                time_part[..8.min(time_part.len())].to_string()
            }
        } else {
            datetime.to_string()
        }
    } else {
        // Fallback - show last 8 characters if it looks like time
        if datetime.len() >= 8 && datetime.contains(':') {
            datetime[datetime.len() - 8..].to_string()
        } else {
            truncate_text(datetime, 8)
        }
    }
}

fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}…", &text[..max_len.saturating_sub(1)])
    }
}

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