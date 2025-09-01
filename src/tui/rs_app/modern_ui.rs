use crate::tui::rs_app::app::App;
use crate::tui::table_ui::TuiTableState;
use crate::tui::theme::{NaviTheme, ResourceStatus, Symbols, TextType, UiConstants, UiHelpers};
use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, 
    ScrollbarOrientation, Wrap
};

/// Modern card-based UI for `ReplicaSet` view
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
        Constraint::Length(UiConstants::ICON_COLUMN_WIDTH),  // Icon + Title
        Constraint::Min(0),      // Namespace info (flexible)
        Constraint::Length(UiConstants::ACTIONS_COLUMN_WIDTH),  // Status/Actions
    ]).split(area);
    
    // Title with icon
    let title_text = format!("{} ReplicaSets", Symbols::REPLICASET);
    let title = Paragraph::new(title_text)
        .style(theme.text_style(TextType::Title).bg(theme.bg_primary))
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(title, header_chunks[0]);
    
    // Namespace context
    let namespace_text = format!("namespace: default • {} items", app.get_items().len());
    let namespace = Paragraph::new(namespace_text)
        .style(theme.text_style(TextType::Caption).bg(theme.bg_primary))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(namespace, header_chunks[1]);
    
    // Actions/shortcuts
    let actions_text = "f: filter • c: colors • q: quit";
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
        Constraint::Min(60),    // Main list (flexible, minimum 60 cols)
        Constraint::Length(UiConstants::DETAILS_PANEL_WIDTH), // Details panel
    ]).split(area);
    
    render_replicaset_list(f, app, content_chunks[0], theme);
    render_details_panel(f, app, content_chunks[1], theme);
}

fn render_replicaset_list(f: &mut Frame, app: &App, area: Rect, theme: &NaviTheme) {
    let items = app.get_filtered_items();
    let selected_index = app.state.selected().unwrap_or(0);
    
    // Create card-style content but render as paragraphs instead of a list
    let mut y_offset = 1; // Start after border
    let content_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    
    let filter = app.get_filter();
    let title = if filter.is_empty() {
        "ReplicaSets".to_string()
    } else {
        format!("ReplicaSets (filtered: {filter})")
    };
    
    // Render container block
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border).bg(theme.bg_secondary))
        .title_style(theme.text_style(TextType::Subtitle).bg(theme.bg_secondary))
        .style(Style::default().bg(theme.bg_secondary));
    f.render_widget(block, area);
    
    // Render individual cards
    for (index, rs) in items.iter().enumerate() {
        if y_offset >= content_area.height {
            break; // Don't render beyond visible area
        }
        
        let is_selected = index == selected_index;
        let card_area = Rect {
            x: content_area.x,
            y: content_area.y + y_offset,
            width: content_area.width,
            height: 3.min(content_area.height - y_offset),
        };
        
        render_replicaset_card(f, rs, card_area, is_selected, theme);
        y_offset += 3;
    }
    
    // Render scrollbar
    render_list_scrollbar(f, app, area, theme);
}

fn render_replicaset_card(f: &mut Frame, rs: &crate::tui::data::Rs, area: Rect, is_selected: bool, theme: &NaviTheme) {
    // Parse replica counts from the "P" field (e.g., "2/3")
    let (current_replicas, desired_replicas) = parse_replica_count(&rs.pods);
    
    // Determine status based on replica count
    let status = if current_replicas == desired_replicas && desired_replicas > 0 {
        ResourceStatus::Running
    } else if current_replicas < desired_replicas {
        ResourceStatus::Pending
    } else if current_replicas == 0 {
        ResourceStatus::Unknown
    } else {
        ResourceStatus::Running
    };
    
    let (status_symbol, status_style) = UiHelpers::status_indicator(status, theme);
    
    // Create health-based progress bar for replica status
    let (progress_bar, progress_color) = if desired_replicas > 0 {
        UiHelpers::health_progress_bar(current_replicas, desired_replicas, 10, theme)
    } else {
        ("──────────".to_string(), theme.text_muted)
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
            Span::styled(&rs.name, theme.text_style(TextType::Title)),
            Span::raw("  "),
            Span::styled(&rs.age, theme.text_style(TextType::Caption)),
        ]),
        Line::from(vec![
            Span::raw("    Replicas: "),
            Span::styled(format!("{current_replicas}/{desired_replicas} "), 
                        theme.text_style(TextType::Body)),
            Span::styled(progress_bar, Style::default().fg(progress_color)),
        ]),
        Line::from(vec![
            Span::raw("    "),
            Span::styled(truncate_text(&rs.description, 50), theme.text_style(TextType::Caption)),
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
    let labels = app.get_left_details();
    
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
                .title(format!("{} Labels", Symbols::BULLET))
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
                .title(format!("{} Events", Symbols::WARNING))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border).bg(theme.bg_tertiary))
                .title_style(theme.text_style(TextType::Subtitle).bg(theme.bg_tertiary))
                .style(Style::default().bg(theme.bg_tertiary))
        );
    
    f.render_widget(events_list, area);
}

fn render_list_scrollbar(f: &mut Frame, app: &App, area: Rect, theme: &NaviTheme) {
    if app.get_items().len() > area.height as usize - 2 {
        f.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(theme.border))
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
            area.inner(Margin { vertical: 1, horizontal: 0 }),
            &mut app.scroll_state.clone(),
        );
    }
}

fn render_footer(f: &mut Frame, area: Rect, theme: &NaviTheme) {
    let footer_text = "Enter: Pods • i: Ingress • e: Events • ↑↓: Navigate • Ctrl+F: Page Down • Ctrl+B: Page Up";
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
                .title(format!("{} Filter ReplicaSets", Symbols::CHEVRON_RIGHT))
                .title_style(theme.text_style(TextType::Subtitle).bg(theme.bg_secondary))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_focus).bg(theme.bg_secondary).add_modifier(Modifier::BOLD))
                .style(Style::default().bg(theme.bg_secondary))
        )
        .wrap(Wrap { trim: true });
    
    f.render_widget(filter_input, modal_area);
    
    // Set cursor position
    let cursor_pos = Position {
        x: modal_area.x + UiHelpers::safe_cast_u16(app.get_cursor_pos(), "rs cursor position") + 1,
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
    
    let help_text = "ESC: Cancel • Enter: Apply • Examples: 'web', '(api|frontend)', 'prod.*db'";
    let help = Paragraph::new(help_text)
        .style(theme.text_style(TextType::Caption).bg(theme.bg_primary))
        .alignment(Alignment::Center)
        .block(Block::default().style(Style::default().bg(theme.bg_primary)));
    
    f.render_widget(help, help_area);
}

// Helper functions

fn parse_replica_count(pods_str: &str) -> (usize, usize) {
    pods_str.find('/').map_or((0, 0), |slash_pos| {
        let current = pods_str[..slash_pos].parse().unwrap_or(0);
        let desired = pods_str[slash_pos + 1..].parse().unwrap_or(0);
        (current, desired)
    })
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