use crate::tui::pod_app::app::App;
use crate::tui::table_ui::TuiTableState;
use crate::tui::theme::{NaviTheme, ResourceStatus, Symbols, TextType, UiConstants, UiHelpers};
use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, 
    ScrollbarOrientation, Wrap
};

const POD_CARD_HEIGHT: u16 = 4;

/// Modern card-based UI for Pod view with container health focus
pub fn ui(f: &mut Frame, app: &mut App) {
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
        Constraint::Length(15),  // Icon + Title
        Constraint::Min(0),      // Context info (flexible)
        Constraint::Length(UiConstants::ACTIONS_COLUMN_WIDTH),  // Actions
    ]).split(area);
    
    // Title with icon
    let title_text = format!("{} Pods", Symbols::POD);
    let title = Paragraph::new(title_text)
        .style(theme.text_style(TextType::Title).bg(theme.bg_primary))
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(title, header_chunks[0]);
    
    // Context info (namespace + pod counts)
    let pods = app.get_items();
    let running_count = pods.iter().filter(|p| p.status() == "Running").count();
    let total_count = pods.len();
    let context_text = format!("namespace: default • {running_count}/{total_count} running");
    let context = Paragraph::new(context_text)
        .style(theme.text_style(TextType::Caption).bg(theme.bg_primary))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(context, header_chunks[1]);
    
    // Actions/shortcuts
    let actions_text = "f: filter • Enter: containers • c: colors • q: quit";
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

fn render_content(f: &mut Frame, app: &mut App, area: Rect, theme: &NaviTheme) {
    let content_chunks = Layout::horizontal([
        Constraint::Min(65),    // Main list (flexible, minimum 65 cols)
        Constraint::Length(UiConstants::DETAILS_PANEL_WIDTH), // Details panel (fixed cols)
    ]).split(area);
    
    render_pod_list(f, app, content_chunks[0], theme);
    render_details_panel(f, app, content_chunks[1], theme);
}

fn render_pod_list(f: &mut Frame, app: &App, area: Rect, theme: &NaviTheme) {
    let items = app.get_filtered_items();
    let selected_index = app.state.selected().unwrap_or(0);
    
    let content_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    
    let filter = app.get_filter();
    let title = if filter.is_empty() {
        "Pods".to_string()
    } else {
        format!("Pods (filtered: {filter})")
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
    let visible_cards = content_area.height / UiConstants::CARD_HEIGHT;
    let selected_index_u16 = UiHelpers::safe_cast_u16(selected_index, "pod_scroll_offset");
    let scroll_offset = if selected_index_u16 >= visible_cards {
        selected_index_u16 - visible_cards + 1
    } else {
        0
    };
    
    // Render individual pod cards with scroll offset
    let mut y_offset = 0;
    for (index, pod) in items.iter().enumerate().skip(scroll_offset as usize) {
        if y_offset + UiConstants::CARD_HEIGHT > content_area.height {
            break; // Don't render beyond visible area
        }
        
        let is_selected = index == selected_index;
        let card_area = Rect {
            x: content_area.x,
            y: content_area.y + y_offset,
            width: content_area.width,
            height: UiConstants::CARD_HEIGHT.min(content_area.height - y_offset),
        };
        
        render_pod_card(f, pod, card_area, is_selected, theme);
        y_offset += UiConstants::CARD_HEIGHT;
    }
    
    // Render scrollbar
    render_list_scrollbar(f, app, area, theme);
}

fn render_pod_card(f: &mut Frame, pod: &crate::tui::data::RsPod, area: Rect, is_selected: bool, theme: &NaviTheme) {
    // Parse container status (e.g., "2/2" means 2 ready out of 2 total)
    let (ready_containers, total_containers) = parse_container_count(&pod.containers);
    
    // Determine pod status
    let pod_status = determine_pod_status(pod.status(), ready_containers, total_containers);
    let (status_symbol, status_style) = UiHelpers::status_indicator(pod_status, theme);
    
    // Create container health bar
    let (container_bar, container_color) = if total_containers > 0 {
        UiHelpers::health_progress_bar(ready_containers, total_containers, 8, theme)
    } else {
        ("────────".to_string(), theme.text_muted)
    };
    
    // Card background - ensure proper contrast
    let card_bg = if is_selected { theme.bg_accent } else { theme.bg_tertiary };
    let selection_indicator = if is_selected { "▶ " } else { "  " };
    
    // Create card content as multi-line text
    let content = vec![
        Line::from(vec![
            Span::raw(selection_indicator),
            Span::styled(status_symbol, status_style),
            Span::raw(" "),
            Span::styled(&pod.name, theme.text_style(TextType::Title)),
            Span::raw("  "),
            Span::styled(&pod.age, theme.text_style(TextType::Caption)),
        ]),
        Line::from(vec![
            Span::raw("    Status: "),
            Span::styled(&pod.status, get_status_style(&pod.status, theme)),
            Span::raw("  Containers: "),
            Span::styled(format!("{ready_containers}/{total_containers} "), 
                        theme.text_style(TextType::Body)),
            Span::styled(container_bar, Style::default().fg(container_color)),
        ]),
        Line::from(vec![
            Span::raw("    "),
            Span::styled(truncate_text(&pod.description, 60), theme.text_style(TextType::Caption)),
        ]),
        Line::from(vec![
            // Add spacing line for card separation
            Span::raw(""),
        ]),
    ];
    
    let card = Paragraph::new(content)
        .style(Style::default().bg(card_bg));
    
    f.render_widget(card, area);
}

fn render_details_panel(f: &mut Frame, app: &mut App, area: Rect, theme: &NaviTheme) {
    let detail_chunks = Layout::vertical([
        Constraint::Min(0),     // Labels section (flexible)
        Constraint::Length(1),  // Divider
        Constraint::Min(0),     // Events section (flexible)
    ]).split(area);
    
    render_labels_section(f, app, detail_chunks[0], theme);
    
    // Horizontal divider
    let divider = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme.divider));
    f.render_widget(divider, detail_chunks[1]);
    
    render_events_section(f, app, detail_chunks[2], theme);
}

fn render_labels_section(f: &mut Frame, app: &mut App, area: Rect, theme: &NaviTheme) {
    let labels = app.get_label_details();
    
    let label_items: Vec<ListItem> = labels
        .iter()
        .map(|(key, value, _)| {
            let content = Line::from(vec![
                Span::styled(format!("{key}: "), theme.text_style(TextType::Body)),
                Span::styled(value, theme.text_style(TextType::Caption)),
            ]);
            ListItem::new(content)
        })
        .collect();
    
    let labels_list = List::new(label_items)
        .block(
            Block::default()
                .title(format!("{} Pod Labels", Symbols::BULLET))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border).bg(theme.bg_tertiary))
                .title_style(theme.text_style(TextType::Subtitle).bg(theme.bg_tertiary))
                .style(Style::default().bg(theme.bg_tertiary))
        );
    
    f.render_widget(labels_list, area);
}

fn render_events_section(f: &mut Frame, app: &mut App, area: Rect, theme: &NaviTheme) {
    let events = app.get_event_details();
    
    let event_items: Vec<ListItem> = events
        .iter()
        .map(|(event_type, message, age)| {
            let type_style = match event_type.as_str() {
                "Warning" => theme.text_style(TextType::Warning),
                "Error" => theme.text_style(TextType::Error),
                _ => theme.text_style(TextType::Success),
            };
            
            let content = vec![
                Line::from(vec![
                    Span::styled(event_type, type_style),
                    Span::raw(" "),
                    Span::styled(age.as_ref().map_or("", |s| s), 
                               theme.text_style(TextType::Caption)),
                ]),
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(truncate_text(message, 35), theme.text_style(TextType::Body)),
                ]),
            ];
            
            ListItem::new(content)
        })
        .collect();
    
    let events_list = List::new(event_items)
        .block(
            Block::default()
                .title(format!("{} Pod Events", Symbols::WARNING))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border).bg(theme.bg_tertiary))
                .title_style(theme.text_style(TextType::Subtitle).bg(theme.bg_tertiary))
                .style(Style::default().bg(theme.bg_tertiary))
        );
    
    f.render_widget(events_list, area);
}

fn render_list_scrollbar(f: &mut Frame, app: &App, area: Rect, theme: &NaviTheme) {
    let items = app.get_filtered_items();
    let content_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    let visible_cards = content_area.height / POD_CARD_HEIGHT;
    
    // Show scrollbar if we have more items than can fit
    if items.len() > visible_cards as usize {
        let selected_index = app.state.selected().unwrap_or(0);
        
        // Calculate scrollbar position based on selection
        let mut scrollbar_state = ratatui::widgets::ScrollbarState::new(items.len().saturating_sub(visible_cards as usize))
            .position(selected_index.saturating_sub(visible_cards as usize / 2));
        
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
    let footer_text = "Enter: Containers • e: Events • ↑↓: Navigate • Ctrl+F: Page Down • Ctrl+B: Page Up";
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
    let modal_area = centered_rect(60, 20, area);
    
    // Clear background
    f.render_widget(Clear, modal_area);
    
    // Modal content
    let filter_text = if app.get_filter().is_empty() {
        "Enter filter pattern...".to_string()
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
                .title(format!("{} Filter Pods", Symbols::CHEVRON_RIGHT))
                .title_style(theme.text_style(TextType::Subtitle).bg(theme.bg_secondary))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_focus).bg(theme.bg_secondary).add_modifier(Modifier::BOLD))
                .style(Style::default().bg(theme.bg_secondary))
        )
        .wrap(Wrap { trim: true });
    
    f.render_widget(filter_input, modal_area);
    
    // Set cursor position
    let cursor_x = UiHelpers::safe_cast_u16(app.get_cursor_pos(), "pod_filter_cursor_position");
    let cursor_pos = Position {
        x: modal_area.x + cursor_x + 1,
        y: modal_area.y + 1,
    };
    f.set_cursor_position(cursor_pos);
    
    // Help text
    let help_area = Rect {
        x: modal_area.x,
        y: modal_area.y + modal_area.height,
        width: modal_area.width,
        height: 1,
    };
    
    let help_text = "ESC: Cancel • Enter: Apply • Examples: 'web-*', '.*failed.*', '^prod-'";
    let help = Paragraph::new(help_text)
        .style(theme.text_style(TextType::Caption).bg(theme.bg_primary))
        .alignment(Alignment::Center)
        .block(Block::default().style(Style::default().bg(theme.bg_primary)));
    
    f.render_widget(help, help_area);
}

// Helper functions

/// Parse container status string (e.g., "2/2" -> (2, 2))
fn parse_container_count(containers_str: &str) -> (usize, usize) {
    containers_str.find('/').map_or_else(|| {
        // Handle cases like "1" (assume 1/1)
        let count = containers_str.parse().unwrap_or(0);
        (count, count)
    }, |slash_pos| {
        let ready = containers_str[..slash_pos].parse().unwrap_or(0);
        let total = containers_str[slash_pos + 1..].parse().unwrap_or(0);
        (ready, total)
    })
}

/// Determine pod status based on Kubernetes pod phase and container readiness
fn determine_pod_status(status: &str, ready_containers: usize, total_containers: usize) -> ResourceStatus {
    match status {
        "Running" => {
            if ready_containers == total_containers && total_containers > 0 {
                ResourceStatus::Running
            } else {
                ResourceStatus::Pending // Some containers not ready
            }
        }
        "Pending" | "ContainerCreating" | "PodInitializing" => ResourceStatus::Pending,
        "Succeeded" | "Completed" => ResourceStatus::Ready,
        "Failed" | "Error" | "CrashLoopBackOff" | "ImagePullBackOff" => ResourceStatus::Failed,
        _ => ResourceStatus::Unknown,
    }
}

/// Get appropriate text style for pod status
fn get_status_style(status: &str, theme: &NaviTheme) -> Style {
    match status {
        "Pending" | "ContainerCreating" | "PodInitializing" => theme.text_style(TextType::Warning),
        "Failed" | "Error" | "CrashLoopBackOff" | "ImagePullBackOff" => theme.text_style(TextType::Error),
        "Running" | "Succeeded" | "Completed" => theme.text_style(TextType::Success),
        _ => theme.text_style(TextType::Caption),
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