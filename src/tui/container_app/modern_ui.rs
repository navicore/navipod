use crate::tui::container_app::app::App;
use crate::tui::table_ui::TuiTableState;
use crate::tui::theme::{NaviTheme, ResourceStatus, Symbols, TextType, UiConstants, UiHelpers};
use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, 
    ScrollbarOrientation, Wrap
};

const CONTAINER_CARD_HEIGHT: u16 = 5;

/// Modern card-based UI for Container view with container runtime focus
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
        Constraint::Min(0),      // Context info (flexible)
        Constraint::Length(UiConstants::ACTIONS_COLUMN_WIDTH),  // Actions
    ]).split(area);
    
    // Title with icon
    let title_text = format!("{} Containers", Symbols::CONTAINER);
    let title = Paragraph::new(title_text)
        .style(theme.text_style(TextType::Title).bg(theme.bg_primary))
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(title, header_chunks[0]);
    
    // Context info (container counts and status)
    let containers = app.get_items();
    let total_count = containers.len();
    let restart_count = containers.iter().filter(|c| {
        UiHelpers::safe_parse_i32(c.restarts(), "container_restarts") > 0
    }).count();
    
    let context_text = if restart_count > 0 {
        format!("{total_count} containers • {restart_count} with restarts")
    } else {
        format!("{total_count} containers")
    };
    
    let context = Paragraph::new(context_text)
        .style(theme.text_style(TextType::Caption).bg(theme.bg_primary))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(context, header_chunks[1]);
    
    // Actions/shortcuts
    let actions_text = "f: filter • Enter: logs • c: colors • q: quit";
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
        Constraint::Min(UiConstants::MIN_LIST_WIDTH),    // Main list (flexible, minimum cols)
        Constraint::Length(UiConstants::DETAILS_PANEL_WIDTH), // Details panel (fixed cols)
    ]).split(area);
    
    render_container_list(f, app, content_chunks[0], theme);
    render_details_panel(f, app, content_chunks[1], theme);
}

fn render_container_list(f: &mut Frame, app: &App, area: Rect, theme: &NaviTheme) {
    let items = app.get_filtered_items();
    let selected_index = app.base.state.selected().unwrap_or(0);
    
    let content_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    
    let filter = app.get_filter();
    let title = if filter.is_empty() {
        "Containers".to_string()
    } else {
        format!("Containers (filtered: {filter})")
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
    let card_height = UiConstants::CARD_HEIGHT + 1; // Containers need more lines for image info
    let visible_cards = content_area.height / card_height;
    let selected_index_u16 = UiHelpers::safe_cast_u16(selected_index, "container_scroll_offset");
    let scroll_offset = if selected_index_u16 >= visible_cards {
        selected_index_u16 - visible_cards + 1
    } else {
        0
    };
    
    // Render individual container cards with scroll offset
    let mut y_offset = 0;
    for (index, container) in items.iter().enumerate().skip(scroll_offset as usize) {
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
        
        render_container_card(f, container, card_area, is_selected, theme);
        y_offset += UiConstants::CARD_HEIGHT;
    }
    
    // Render scrollbar
    render_list_scrollbar(f, app, area, theme);
}

fn render_container_card(f: &mut Frame, container: &crate::tui::data::Container, area: Rect, is_selected: bool, theme: &NaviTheme) {
    // Parse restart count to determine container health
    let restart_count = container.restarts().parse::<i32>().unwrap_or(0);
    let container_status = determine_container_status(restart_count);
    let (status_symbol, status_style) = UiHelpers::status_indicator(container_status, theme);
    
    // Create restart health bar - higher restarts = worse health
    let (restart_bar, restart_color) = if restart_count > 0 {
        let health_ratio = if restart_count <= 2 { 0.75 } else if restart_count <= 5 { 0.5 } else if restart_count <= 10 { 0.25 } else { 0.0 };
        UiHelpers::health_progress_bar(
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Intentional for progress bar display
            if health_ratio > 0.0 { (health_ratio * 8.0) as usize } else { 0 },
            8,
            8,
            theme
        )
    } else {
        ("▓▓▓▓▓▓▓▓".to_string(), theme.success) // Perfect health for no restarts
    };
    
    // Card background - ensure proper contrast
    let card_bg = if is_selected { theme.bg_accent } else { theme.bg_tertiary };
    let selection_indicator = if is_selected { "▶ " } else { "  " };
    
    // Extract short image name (remove registry/tag for display)
    let short_image = extract_image_name(container.image());
    
    // Create card content as multi-line text
    let content = vec![
        Line::from(vec![
            Span::raw(selection_indicator),
            Span::styled(status_symbol, status_style),
            Span::raw(" "),
            Span::styled(&container.name, theme.text_style(TextType::Title)),
            Span::raw("  "),
            Span::styled(format!("Pod: {}", &container.pod_name), theme.text_style(TextType::Caption)),
        ]),
        Line::from(vec![
            Span::raw("    Image: "),
            Span::styled(short_image, theme.text_style(TextType::Body)),
        ]),
        Line::from(vec![
            Span::raw("    Ports: "),
            Span::styled(container.ports(), theme.text_style(TextType::Body)),
            Span::raw("  Restarts: "),
            Span::styled(format!("{restart_count} "), get_restart_style(restart_count, theme)),
            Span::styled(restart_bar, Style::default().fg(restart_color)),
        ]),
        Line::from(vec![
            Span::raw("    "),
            Span::styled(truncate_text(container.description(), 60), theme.text_style(TextType::Caption)),
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
        Constraint::Min(0),     // Mounts section (flexible)
        Constraint::Length(1),  // Divider
        Constraint::Min(0),     // Env vars section (flexible)
        Constraint::Length(1),  // Divider
        Constraint::Min(0),     // Probes section (flexible)
    ]).split(area);
    
    render_mounts_section(f, app, detail_chunks[0], theme);
    
    // Horizontal divider
    let divider = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme.divider));
    f.render_widget(divider, detail_chunks[1]);
    
    render_env_vars_section(f, app, detail_chunks[2], theme);
    
    // Horizontal divider
    let divider = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme.divider));
    f.render_widget(divider, detail_chunks[3]);
    
    render_probes_section(f, app, detail_chunks[4], theme);
}

fn render_mounts_section(f: &mut Frame, app: &mut App, area: Rect, theme: &NaviTheme) {
    let mounts = app.get_left_details();
    
    let mount_items: Vec<ListItem> = mounts
        .iter()
        .map(|(name, path, _)| {
            let content = Line::from(vec![
                Span::styled(format!("{name}: "), theme.text_style(TextType::Body)),
                Span::styled(path, theme.text_style(TextType::Caption)),
            ]);
            ListItem::new(content)
        })
        .collect();
    
    let mounts_list = List::new(mount_items)
        .block(
            Block::default()
                .title(format!("{} Volume Mounts", Symbols::FOLDER))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border).bg(theme.bg_tertiary))
                .title_style(theme.text_style(TextType::Subtitle).bg(theme.bg_tertiary))
                .style(Style::default().bg(theme.bg_tertiary))
        );
    
    f.render_widget(mounts_list, area);
}

fn render_env_vars_section(f: &mut Frame, app: &mut App, area: Rect, theme: &NaviTheme) {
    let env_vars = app.get_right_details();
    
    let env_items: Vec<ListItem> = env_vars
        .iter()
        .map(|(key, value, _)| {
            let content = Line::from(vec![
                Span::styled(format!("{key}: "), theme.text_style(TextType::Body)),
                Span::styled(truncate_text(value, 25), theme.text_style(TextType::Caption)),
            ]);
            ListItem::new(content)
        })
        .collect();
    
    let env_list = List::new(env_items)
        .block(
            Block::default()
                .title(format!("{} Environment Variables", Symbols::SETTINGS))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border).bg(theme.bg_tertiary))
                .title_style(theme.text_style(TextType::Subtitle).bg(theme.bg_tertiary))
                .style(Style::default().bg(theme.bg_tertiary))
        );
    
    f.render_widget(env_list, area);
}

fn render_probes_section(f: &mut Frame, app: &mut App, area: Rect, theme: &NaviTheme) {
    let probes = if let Some(selected_container) = app.get_selected_item() {
        &selected_container.probes
    } else {
        return; // No container selected
    };
    
    let probe_items: Vec<ListItem> = probes
        .iter()
        .map(|probe| {
            let handler_symbol = match probe.handler_type.as_str() {
                "HTTP" => "🌐",
                "TCP" => "🔌",
                "Exec" => "⚡",
                _ => "❓",
            };
            
            let probe_type_color = match probe.probe_type.as_str() {
                "Liveness" => theme.error,   // Red for critical liveness
                "Readiness" => theme.warning, // Yellow for readiness
                "Startup" => theme.info,     // Blue for startup
                _ => theme.text_primary,
            };
            
            let content = Line::from(vec![
                Span::raw(format!("{handler_symbol} ")),
                Span::styled(format!("{}: ", probe.probe_type), Style::default().fg(probe_type_color).add_modifier(Modifier::BOLD)),
                Span::styled(&probe.details, theme.text_style(TextType::Caption)),
            ]);
            ListItem::new(content)
        })
        .collect();
    
    // Show message if no probes configured
    let probe_list = if probe_items.is_empty() {
        let empty_content = Line::from(vec![
            Span::styled("No health probes configured", theme.text_style(TextType::Caption).add_modifier(Modifier::ITALIC))
        ]);
        List::new(vec![ListItem::new(empty_content)])
    } else {
        List::new(probe_items)
    };
    
    let probes_widget = probe_list
        .block(
            Block::default()
                .title("🏥 Health Probes")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border).bg(theme.bg_tertiary))
                .title_style(theme.text_style(TextType::Subtitle).bg(theme.bg_tertiary))
                .style(Style::default().bg(theme.bg_tertiary))
        );
    
    f.render_widget(probes_widget, area);
}

fn render_list_scrollbar(f: &mut Frame, app: &App, area: Rect, theme: &NaviTheme) {
    let items = app.get_filtered_items();
    let content_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    let visible_cards = content_area.height / CONTAINER_CARD_HEIGHT;
    
    // Show scrollbar if we have more items than can fit
    if items.len() > visible_cards as usize {
        let selected_index = app.base.state.selected().unwrap_or(0);
        
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
    let footer_text = "Enter: Logs • e: Environment • m: Mounts • p: Probes • ↑↓: Navigate • Ctrl+F: Page Down • Ctrl+B: Page Up";
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
                .title(format!("{} Filter Containers", Symbols::CHEVRON_RIGHT))
                .title_style(theme.text_style(TextType::Subtitle).bg(theme.bg_secondary))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_focus).bg(theme.bg_secondary).add_modifier(Modifier::BOLD))
                .style(Style::default().bg(theme.bg_secondary))
        )
        .wrap(Wrap { trim: true });
    
    f.render_widget(filter_input, modal_area);
    
    // Set cursor position
    let cursor_x = UiHelpers::safe_cast_u16(app.get_cursor_pos(), "filter_cursor_position");
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
    
    let help_text = "ESC: Cancel • Enter: Apply • Examples: 'web-*', '.*api.*', '^prod-'";
    let help = Paragraph::new(help_text)
        .style(theme.text_style(TextType::Caption).bg(theme.bg_primary))
        .alignment(Alignment::Center)
        .block(Block::default().style(Style::default().bg(theme.bg_primary)));
    
    f.render_widget(help, help_area);
}

// Helper functions

/// Determine container status based on restart count
const fn determine_container_status(restart_count: i32) -> ResourceStatus {
    match restart_count {
        0 => ResourceStatus::Running,           // Perfect - no restarts
        1..=2 => ResourceStatus::Ready,        // Acceptable - minimal restarts
        3..=5 => ResourceStatus::Pending,      // Warning - moderate restarts
        6..=10 => ResourceStatus::Unknown,     // Concerning - high restarts
        _ => ResourceStatus::Failed,           // Critical - excessive restarts
    }
}

/// Get appropriate text style for restart count
fn get_restart_style(restart_count: i32, theme: &NaviTheme) -> Style {
    match restart_count {
        0 => theme.text_style(TextType::Success),
        1..=2 => theme.text_style(TextType::Body),
        3..=5 => theme.text_style(TextType::Warning),
        _ => theme.text_style(TextType::Error),
    }
}

/// Extract image name with tag from full image path (removes registry/namespace)
fn extract_image_name(full_image: &str) -> String {
    // Handle registry.io/namespace/image:tag format
    let parts: Vec<&str> = full_image.split('/').collect();
    let image_with_tag = parts.last().unwrap_or(&full_image);
    
    // Keep the image name WITH tag (this is the change - don't split on ':')
    // Truncate if still too long (increased length to accommodate tags)
    truncate_text(image_with_tag, 35)
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