#![allow(clippy::match_same_arms)] // Some match arms are intentionally the same

use crate::tui::container_app::app::App;
use crate::tui::table_ui::TuiTableState;
use crate::tui::theme::{NaviTheme, ResourceStatus, Symbols, TextType, UiConstants, UiHelpers};
use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar, 
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
    
    // Show probe results popup if active
    if app.show_probe_popup {
        render_probe_results_popup(f, app, &theme);
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
    let actions_text = "TAB: panels • f: filter • Enter: execute • q: quit";
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

    // Calculate CPU and memory info
    let cpu_display = format_resource_display(container.cpu_usage.as_ref(), container.cpu_limit.as_ref());
    let cpu_percentage = calculate_resource_percentage(container.cpu_usage.as_ref(), container.cpu_limit.as_ref());
    let cpu_bar_color = get_resource_bar_color(cpu_percentage, theme);
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    let (cpu_bar, _) = cpu_percentage.map_or_else(
        || ("────────".to_string(), theme.text_muted),
        |pct| UiHelpers::health_progress_bar((pct as usize).min(100), 100, 8, theme)
    );

    let memory_display = format_resource_display(container.memory_usage.as_ref(), container.memory_limit.as_ref());
    let memory_percentage = calculate_resource_percentage(container.memory_usage.as_ref(), container.memory_limit.as_ref());
    let memory_bar_color = get_resource_bar_color(memory_percentage, theme);
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    let (memory_bar, _) = memory_percentage.map_or_else(
        || ("────────".to_string(), theme.text_muted),
        |pct| UiHelpers::health_progress_bar((pct as usize).min(100), 100, 8, theme)
    );

    // Create card content as multi-line text
    let mut content = vec![
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
    ];

    // Add CPU line if we have data
    if container.cpu_usage.is_some() || container.cpu_limit.is_some() {
        content.push(Line::from(vec![
            Span::raw("    CPU: "),
            Span::styled(&cpu_display, theme.text_style(TextType::Body)),
            Span::raw(" "),
            Span::styled(cpu_bar, Style::default().fg(cpu_bar_color)),
        ]));
    }

    // Add memory line if we have data
    if container.memory_usage.is_some() || container.memory_limit.is_some() {
        content.push(Line::from(vec![
            Span::raw("    Mem: "),
            Span::styled(&memory_display, theme.text_style(TextType::Body)),
            Span::raw(" "),
            Span::styled(memory_bar, Style::default().fg(memory_bar_color)),
        ]));
    }

    content.push(Line::from(vec![
        Span::raw("    "),
        Span::styled(truncate_text(container.description(), 60), theme.text_style(TextType::Caption)),
    ]));

    content.push(Line::from(vec![
        // Add spacing line for card separation
        Span::raw(""),
    ]));
    
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
    let is_focused = app.focused_panel == crate::tui::container_app::app::FocusedPanel::Mounts;
    
    let scroll_offset = if is_focused { app.detail_scroll_offset } else { 0 };
    let visible_mounts: Vec<_> = mounts
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(10) // Show up to 10 visible items
        .collect();
    
    let mount_items: Vec<ListItem> = visible_mounts
        .iter()
        .map(|(absolute_index, (name, path, _))| {
            let is_selected = is_focused && *absolute_index == app.detail_selection;
            let selection_indicator = if is_selected { "▶ " } else { "  " };
            
            let content = Line::from(vec![
                Span::raw(selection_indicator),
                Span::styled(format!("{name}: "), theme.text_style(TextType::Body)),
                Span::styled(path, theme.text_style(TextType::Caption)),
            ]);
            
            let item = ListItem::new(content);
            if is_selected {
                item.style(Style::default().bg(theme.bg_accent))
            } else {
                item
            }
        })
        .collect();
    
    let border_style = if is_focused {
        Style::default().fg(theme.border_focus).bg(theme.bg_tertiary).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.border).bg(theme.bg_tertiary)
    };
    
    let title = if is_focused {
        format!("{} Volume Mounts [TAB: next panel]", Symbols::FOLDER)
    } else {
        format!("{} Volume Mounts", Symbols::FOLDER)
    };
    
    let mounts_list = List::new(mount_items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style)
                .title_style(theme.text_style(TextType::Subtitle).bg(theme.bg_tertiary))
                .style(Style::default().bg(theme.bg_tertiary))
        );
    
    f.render_widget(mounts_list, area);
}

fn render_env_vars_section(f: &mut Frame, app: &mut App, area: Rect, theme: &NaviTheme) {
    let env_vars = app.get_right_details();
    let is_focused = app.focused_panel == crate::tui::container_app::app::FocusedPanel::EnvVars;
    
    let scroll_offset = if is_focused { app.detail_scroll_offset } else { 0 };
    let visible_env_vars: Vec<_> = env_vars
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(10) // Show up to 10 visible items
        .collect();
    
    let env_items: Vec<ListItem> = visible_env_vars
        .iter()
        .map(|(absolute_index, (key, value, _))| {
            let is_selected = is_focused && *absolute_index == app.detail_selection;
            let selection_indicator = if is_selected { "▶ " } else { "  " };
            
            let content = Line::from(vec![
                Span::raw(selection_indicator),
                Span::styled(format!("{key}: "), theme.text_style(TextType::Body)),
                Span::styled(truncate_text(value, 25), theme.text_style(TextType::Caption)),
            ]);
            
            let item = ListItem::new(content);
            if is_selected {
                item.style(Style::default().bg(theme.bg_accent))
            } else {
                item
            }
        })
        .collect();
    
    let border_style = if is_focused {
        Style::default().fg(theme.border_focus).bg(theme.bg_tertiary).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.border).bg(theme.bg_tertiary)
    };
    
    let title = if is_focused {
        format!("{} Environment Variables [TAB: next panel]", Symbols::SETTINGS)
    } else {
        format!("{} Environment Variables", Symbols::SETTINGS)
    };
    
    let env_list = List::new(env_items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style)
                .title_style(theme.text_style(TextType::Subtitle).bg(theme.bg_tertiary))
                .style(Style::default().bg(theme.bg_tertiary))
        );
    
    f.render_widget(env_list, area);
}

fn render_probes_section(f: &mut Frame, app: &mut App, area: Rect, theme: &NaviTheme) {
    let selected_container = app.get_selected_item().cloned();
    let is_focused = app.focused_panel == crate::tui::container_app::app::FocusedPanel::Probes;
    
    let probes = if let Some(ref container) = selected_container {
        &container.probes
    } else {
        return; // No container selected
    };
    
    // Show probe configurations with selection and scrolling
    let probe_items: Vec<ListItem> = if probes.is_empty() {
        vec![ListItem::new(Line::from(vec![
            Span::styled("  No health probes configured", theme.text_style(TextType::Caption).add_modifier(Modifier::ITALIC))
        ]))]
    } else {
        // Don't manually slice - let ListState handle scrolling and selection
        probes
            .iter()
            .enumerate()
            .map(|(index, probe)| {
                let is_selected = is_focused && index == app.detail_selection;
                let selection_indicator = if is_selected { "▶ " } else { "  " };
                
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
                    Span::raw(selection_indicator),
                    Span::raw(format!("{handler_symbol} ")),
                    Span::styled(format!("{}: ", probe.probe_type), Style::default().fg(probe_type_color).add_modifier(Modifier::BOLD)),
                    Span::styled(&probe.details, theme.text_style(TextType::Caption)),
                ]);
                
                ListItem::new(content)
            })
            .collect()
    };
    
    let border_style = if is_focused {
        Style::default().fg(theme.border_focus).bg(theme.bg_tertiary).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.border).bg(theme.bg_tertiary)
    };
    
    let title = if is_focused {
        "🏥 Health Probes [ENTER: execute • TAB: next panel]"
    } else {
        "🏥 Health Probes"
    };
    
    // Create the list widget
    let probes_widget = List::new(probe_items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style)
                .title_style(theme.text_style(TextType::Subtitle).bg(theme.bg_tertiary))
                .style(Style::default().bg(theme.bg_tertiary))
        );
    
    // Create list state for scrolling
    let mut list_state = ListState::default();
    if is_focused && !probes.is_empty() {
        list_state.select(Some(app.detail_selection));
    }
    
    f.render_stateful_widget(probes_widget, area, &mut list_state);
    
    // Add scrollbar if focused and has items that might scroll
    if is_focused && !probes.is_empty() {
        let scrollbar_area = Rect {
            x: area.right() - 1,
            y: area.top() + 1,
            width: 1,
            height: area.height.saturating_sub(2),
        };
        
        let mut scrollbar_state = ratatui::widgets::ScrollbarState::new(probes.len().saturating_sub(1))
            .position(app.detail_selection);
            
        f.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(theme.border_focus).bg(theme.bg_tertiary))
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"))
                .track_symbol(Some("│"))
                .thumb_symbol("█"),
            scrollbar_area,
            &mut scrollbar_state,
        );
    }
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
    let footer_text = "TAB: Focus panels • Enter: Execute/Logs • ↑↓: Navigate • f: filter • Esc: Back/Close • q: quit";
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

fn render_probe_results_popup(f: &mut Frame, app: &App, theme: &NaviTheme) {
    let area = f.area();
    let modal_area = centered_rect(75, 70, area);
    
    // Clear background
    f.render_widget(Clear, modal_area);
    
    if let Some(ref result) = app.current_probe_result {
        let all_lines = build_probe_result_lines(result, theme);
        let content_height = modal_area.height.saturating_sub(4); // 2 for borders, 2 for padding
        let total_lines = all_lines.len();
        
        // Apply scrolling - skip lines based on scroll position
        let visible_lines: Vec<Line> = all_lines
            .clone()
            .into_iter()
            .skip(app.probe_popup_scroll)
            .take(content_height as usize)
            .collect();
        
        render_probe_popup_content(f, modal_area, visible_lines, theme);
        render_probe_popup_scrollbar(f, modal_area, total_lines, content_height, app.probe_popup_scroll);
        render_probe_popup_help(f, modal_area, theme);
    }
}

fn build_probe_result_lines(result: &crate::k8s::probes::ProbeResult, theme: &NaviTheme) -> Vec<Line<'static>> {
    let mut all_lines = Vec::new();
    
    // Header with probe type and status
    let (status_symbol, status_color) = get_probe_status_display(&result.status, theme);
    
    all_lines.push(Line::from(vec![
        Span::styled(format!("{} {} {} Probe Result", status_symbol, result.probe_type, result.handler_type), 
            Style::default().fg(status_color).add_modifier(Modifier::BOLD))
    ]));
    
    all_lines.push(Line::from(vec![Span::raw("")])); // Empty line
    
    // Execution details
    all_lines.push(Line::from(vec![
        Span::styled("Execution Time: ", theme.text_style(TextType::Body).add_modifier(Modifier::BOLD)),
        Span::styled(format!("{}ms", result.response_time_ms), theme.text_style(TextType::Body)),
    ]));
    
    all_lines.push(Line::from(vec![
        Span::styled("Timestamp: ", theme.text_style(TextType::Body).add_modifier(Modifier::BOLD)),
        Span::styled(result.timestamp.clone(), theme.text_style(TextType::Body)),
    ]));
    
    if let Some(status_code) = result.status_code {
        all_lines.push(Line::from(vec![
            Span::styled("HTTP Status: ", theme.text_style(TextType::Body).add_modifier(Modifier::BOLD)),
            Span::styled(status_code.to_string(), theme.text_style(TextType::Body)),
        ]));
    }
    
    if let Some(ref error) = result.error_message {
        all_lines.push(Line::from(vec![Span::raw("")])); // Empty line
        all_lines.push(Line::from(vec![
            Span::styled("Error: ", Style::default().fg(theme.error).add_modifier(Modifier::BOLD)),
            Span::styled(error.clone(), Style::default().fg(theme.error)),
        ]));
    }
    
    if !result.response_body.is_empty() {
        all_lines.push(Line::from(vec![Span::raw("")])); // Empty line
        all_lines.push(Line::from(vec![
            Span::styled("Response:", theme.text_style(TextType::Body).add_modifier(Modifier::BOLD))
        ]));
        
        // Add ALL response body lines for scrolling
        for line in result.response_body.lines() {
            all_lines.push(Line::from(vec![
                Span::styled(line.to_string(), theme.text_style(TextType::Caption))
            ]));
        }
    }
    
    all_lines
}

const fn get_probe_status_display(status: &crate::k8s::probes::ProbeStatus, theme: &NaviTheme) -> (&'static str, Color) {
    match status {
        crate::k8s::probes::ProbeStatus::Success => ("✅", theme.success),
        crate::k8s::probes::ProbeStatus::Failure => ("❌", theme.error),
        crate::k8s::probes::ProbeStatus::Timeout => ("⏱️", theme.warning),
        crate::k8s::probes::ProbeStatus::Error => ("🚫", theme.error),
    }
}

fn render_probe_popup_content(f: &mut Frame, modal_area: Rect, visible_lines: Vec<Line>, theme: &NaviTheme) {
    let popup = Paragraph::new(visible_lines)
        .style(Style::default().bg(theme.bg_secondary))
        .block(
            Block::default()
                .title("🏥 Probe Execution Result")
                .title_style(theme.text_style(TextType::Subtitle).bg(theme.bg_secondary))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_focus).bg(theme.bg_secondary).add_modifier(Modifier::BOLD))
                .style(Style::default().bg(theme.bg_secondary))
        )
        .wrap(Wrap { trim: true });
        
    f.render_widget(popup, modal_area);
}

fn render_probe_popup_scrollbar(f: &mut Frame, modal_area: Rect, total_lines: usize, content_height: u16, scroll_position: usize) {
    if total_lines > content_height as usize {
        let scrollbar = ratatui::widgets::Scrollbar::new(ratatui::widgets::ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
            
        let mut scrollbar_state = ratatui::widgets::ScrollbarState::new(total_lines)
            .position(scroll_position);
            
        f.render_stateful_widget(
            scrollbar,
            modal_area.inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
}

fn render_probe_popup_help(f: &mut Frame, modal_area: Rect, theme: &NaviTheme) {
    let help_area = Rect {
        x: modal_area.x,
        y: modal_area.y + modal_area.height,
        width: modal_area.width,
        height: 1,
    };
    
    let help_text = "↑↓/j/k: Scroll • g/G: Top/Bottom • q: Quit • ESC: Close";
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

/// Calculate usage percentage for resource
fn calculate_resource_percentage(usage_str: Option<&String>, limit_str: Option<&String>) -> Option<f64> {
    use crate::k8s::resources::{parse_cpu, parse_memory};

    match (usage_str, limit_str) {
        (Some(usage), Some(limit)) => {
            // Try CPU parsing first
            if let (Some(usage_val), Some(limit_val)) = (parse_cpu(usage), parse_cpu(limit)) {
                if limit_val > 0.0 {
                    return Some((usage_val / limit_val) * 100.0);
                }
            }
            // Try memory parsing
            #[allow(clippy::cast_precision_loss)]
            if let (Some(usage_val), Some(limit_val)) = (parse_memory(usage), parse_memory(limit)) {
                if limit_val > 0 {
                    return Some((usage_val as f64 / limit_val as f64) * 100.0);
                }
            }
            None
        }
        _ => None,
    }
}

/// Get resource bar color based on usage percentage
fn get_resource_bar_color(percentage: Option<f64>, theme: &NaviTheme) -> Color {
    match percentage {
        Some(pct) if pct >= 75.0 => theme.error,  // Critical - Red
        Some(pct) if pct >= 60.0 => theme.warning, // Warning - Yellow
        Some(_) => theme.success,                   // Healthy - Green
        None => theme.text_muted,                        // Unknown - Gray
    }
}

/// Format resource display: "usage/limit [percent%]"
fn format_resource_display(usage: Option<&String>, limit: Option<&String>) -> String {
    match (usage, limit) {
        (Some(u), Some(l)) => {
            calculate_resource_percentage(Some(u), Some(l)).map_or_else(
                || format!("{u}/{l}"),
                |pct| format!("{u}/{l} [{pct:.0}%]")
            )
        }
        (Some(u), None) => format!("{u}/∞"),
        (None, Some(l)) => format!("?/{l}"),
        (None, None) => "N/A".to_string(),
    }
}