use crate::tui::ingress_app::app::App;
use crate::tui::table_ui::TuiTableState;
use crate::tui::theme::{NaviTheme, ResourceStatus, Symbols, TextType, UiHelpers};
use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Clear, Paragraph, Scrollbar, 
    ScrollbarOrientation, Wrap
};

/// Modern card-based UI for Ingress view with network routing focus
pub fn ui(f: &mut Frame, app: &mut App) {
    let theme = NaviTheme::default();
    
    // Set the main background to ensure consistent theming
    let main_bg = Block::default()
        .style(Style::default().bg(theme.bg_primary));
    f.render_widget(main_bg, f.area());
    
    // Main layout: header, content, footer
    let main_chunks = Layout::vertical([
        Constraint::Length(3),  // Header
        Constraint::Min(0),     // Content (flexible)
        Constraint::Length(2),  // Footer
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
        Constraint::Length(18),  // Icon + Title
        Constraint::Min(0),      // Context info (flexible)
        Constraint::Length(35),  // Actions
    ]).split(area);
    
    // Title with icon
    let title_text = format!("{} Ingress Routes", Symbols::INGRESS);
    let title = Paragraph::new(title_text)
        .style(theme.text_style(TextType::Title).bg(theme.bg_primary))
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(title, header_chunks[0]);
    
    // Context info (route counts and backend analysis)
    let routes = app.get_items();
    let total_count = routes.len();
    let unique_hosts = routes.iter()
        .map(|r| r.host())
        .collect::<std::collections::HashSet<_>>()
        .len();
    let unique_backends = routes.iter()
        .map(|r| r.backend_svc())
        .collect::<std::collections::HashSet<_>>()
        .len();
    
    let context_text = format!("{} routes • {} hosts • {} backends", 
        total_count, unique_hosts, unique_backends);
    
    let context = Paragraph::new(context_text)
        .style(theme.text_style(TextType::Caption).bg(theme.bg_primary))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(context, header_chunks[1]);
    
    // Actions/shortcuts
    let actions_text = "f: filter • Enter: certificates • c: colors • q: quit";
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
    render_ingress_list(f, app, area, theme);
}

fn render_ingress_list(f: &mut Frame, app: &App, area: Rect, theme: &NaviTheme) {
    let items = app.get_filtered_items();
    let selected_index = app.state.selected().unwrap_or(0);
    
    let content_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    
    let title = if !app.get_filter().is_empty() {
        format!("Network Routes (filtered: {})", app.get_filter())
    } else {
        "Network Routes".to_string()
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
    const CARD_HEIGHT: u16 = 4; // 4 lines per ingress route card
    let visible_cards = content_area.height / CARD_HEIGHT;
    let scroll_offset = if selected_index as u16 >= visible_cards {
        selected_index as u16 - visible_cards + 1
    } else {
        0
    };
    
    // Render individual ingress cards with scroll offset
    let mut y_offset = 0;
    for (index, ingress) in items.iter().enumerate().skip(scroll_offset as usize) {
        if y_offset + CARD_HEIGHT > content_area.height {
            break; // Don't render beyond visible area
        }
        
        let is_selected = index == selected_index;
        let card_area = Rect {
            x: content_area.x,
            y: content_area.y + y_offset,
            width: content_area.width,
            height: CARD_HEIGHT.min(content_area.height - y_offset),
        };
        
        render_ingress_card(f, ingress, card_area, is_selected, theme);
        y_offset += CARD_HEIGHT;
    }
    
    // Render scrollbar
    render_list_scrollbar(f, app, area, theme);
}

fn render_ingress_card(f: &mut Frame, ingress: &crate::tui::data::Ingress, area: Rect, is_selected: bool, theme: &NaviTheme) {
    // Determine route status based on path and backend configuration
    let route_status = determine_route_status(ingress);
    let (status_symbol, status_style) = UiHelpers::status_indicator(route_status, theme);
    
    // Analyze route configuration
    let is_secure = ingress.host().starts_with("https://") || 
                   ingress.host().contains(".tls") ||
                   ingress.port() == "443";
    let protocol_indicator = if is_secure { "🔒 HTTPS" } else { "🌐 HTTP" };
    let protocol_style = if is_secure { 
        theme.text_style(TextType::Success) 
    } else { 
        theme.text_style(TextType::Warning) 
    };
    
    // Card background - ensure proper contrast
    let card_bg = if is_selected { theme.bg_accent } else { theme.bg_tertiary };
    let selection_indicator = if is_selected { "▶ " } else { "  " };
    
    // Create routing flow visualization
    let routing_flow = format!("{} {} {} {}", 
        truncate_text(ingress.host(), 20),
        Symbols::ARROW_RIGHT,
        truncate_text(ingress.backend_svc(), 15),
        if !ingress.port().is_empty() { format!(":{}", ingress.port()) } else { String::new() }
    );
    
    // Create card content as multi-line text
    let content = vec![
        Line::from(vec![
            Span::raw(selection_indicator),
            Span::styled(status_symbol, status_style),
            Span::raw(" "),
            Span::styled(&ingress.name, theme.text_style(TextType::Title)),
            Span::raw("  "),
            Span::styled(protocol_indicator, protocol_style),
        ]),
        Line::from(vec![
            Span::raw("    Route: "),
            Span::styled(routing_flow, theme.text_style(TextType::Body)),
        ]),
        Line::from(vec![
            Span::raw("    Path: "),
            Span::styled(format!("{} ", ingress.path()), theme.text_style(TextType::Caption)),
            Span::raw(" Backend: "),
            Span::styled(format!("{}", ingress.backend_svc()), theme.text_style(TextType::Body)),
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

fn render_list_scrollbar(f: &mut Frame, app: &App, area: Rect, theme: &NaviTheme) {
    let items = app.get_filtered_items();
    const CARD_HEIGHT: u16 = 4;
    let content_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    let visible_cards = content_area.height / CARD_HEIGHT;
    
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
    let footer_text = "Enter: Check SSL certificates • r: Route details • ↑↓: Navigate • Ctrl+F: Page Down • Ctrl+B: Page Up";
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
                .title(format!("{} Filter Ingress Routes", Symbols::CHEVRON_RIGHT))
                .title_style(theme.text_style(TextType::Subtitle).bg(theme.bg_secondary))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_focus).bg(theme.bg_secondary).add_modifier(Modifier::BOLD))
                .style(Style::default().bg(theme.bg_secondary))
        )
        .wrap(Wrap { trim: true });
    
    f.render_widget(filter_input, modal_area);
    
    // Set cursor position
    #[allow(clippy::cast_possible_truncation)]
    let cursor_pos = Position {
        x: modal_area.x + app.get_cursor_pos() as u16 + 1,
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
    
    let help_text = "ESC: Cancel • Enter: Apply • Examples: 'api-*', '.*prod.*', 'web.*service'";
    let help = Paragraph::new(help_text)
        .style(theme.text_style(TextType::Caption).bg(theme.bg_primary))
        .alignment(Alignment::Center)
        .block(Block::default().style(Style::default().bg(theme.bg_primary)));
    
    f.render_widget(help, help_area);
}

// Helper functions

/// Determine route status based on ingress configuration
fn determine_route_status(ingress: &crate::tui::data::Ingress) -> ResourceStatus {
    // Analyze route configuration to determine health
    let has_host = !ingress.host().is_empty() && ingress.host() != "-";
    let has_backend = !ingress.backend_svc().is_empty() && ingress.backend_svc() != "-";
    let has_path = !ingress.path().is_empty() && ingress.path() != "-";
    let has_port = !ingress.port().is_empty() && ingress.port() != "-";
    
    // Check for common issues
    if !has_host || !has_backend {
        ResourceStatus::Failed // Missing critical routing info
    } else if !has_path {
        ResourceStatus::Pending // Missing path configuration  
    } else if !has_port {
        ResourceStatus::Unknown // Port might be optional
    } else if ingress.host().starts_with("https://") || ingress.port() == "443" {
        ResourceStatus::Running // Secure route configured properly
    } else {
        ResourceStatus::Ready // Basic HTTP route configured
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