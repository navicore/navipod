use crate::tui::cert_app::app::App;
use crate::tui::cert_validation::CertificateValidator;
use crate::tui::shared_ui::{SharedComponents, UiCache};
use crate::tui::table_ui::TuiTableState;
use crate::tui::theme::{NaviTheme, ResourceStatus, Symbols, TextType, UiHelpers};
use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Clear, Paragraph, Scrollbar, 
    ScrollbarOrientation, Wrap
};

/// Modern card-based UI for Certificate view with security and SSL focus
/// Uses shared components and improved certificate validation
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
    
    // Use shared footer component
    SharedComponents::render_standard_footer(
        f,
        main_chunks[2],
        "↑/k up • ↓/j down • / filter • c colors • q quit",
        &theme,
    );
    
    // Handle overlays
    if app.get_show_filter_edit() {
        render_filter_modal(f, app, &theme);
    }
}

fn render_header(f: &mut Frame, app: &App, area: Rect, theme: &NaviTheme) {
    let header_chunks = Layout::horizontal([
        Constraint::Length(20),  // Icon + Title
        Constraint::Min(0),      // Context info (flexible)
        Constraint::Length(30),  // Actions
    ]).split(area);
    
    let validator = CertificateValidator::default();
    
    // Title with security icon
    let title_text = "🔒 SSL Certificates";
    let title = Paragraph::new(title_text)
        .style(theme.text_style(TextType::Title).bg(theme.bg_primary))
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(title, header_chunks[0]);
    
    // Context info (certificate status analysis using improved validation)
    let certs = app.get_items();
    let total_count = certs.len();
    let valid_count = certs.iter().filter(|c| c.is_valid() == "true" || c.is_valid().to_lowercase() == "valid").count();
    let expired_count = certs.iter().filter(|c| validator.is_expiring_soon(c.expires())).count();
    
    let context_text = if expired_count > 0 {
        format!("{} certs • {} valid • {} expiring soon ⚠️", total_count, valid_count, expired_count)
    } else {
        format!("{} certificates • {} valid", total_count, valid_count)
    };
    
    let context_style = if expired_count > 0 {
        theme.text_style(TextType::Warning).bg(theme.bg_primary)
    } else {
        theme.text_style(TextType::Caption).bg(theme.bg_primary)
    };
    
    let context = Paragraph::new(context_text)
        .style(context_style)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(context, header_chunks[1]);
    
    // Actions/shortcuts
    let actions_text = "f: filter • r: refresh • c: colors • q: quit";
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
    render_cert_list(f, app, area, theme);
}

fn render_cert_list(f: &mut Frame, app: &App, area: Rect, theme: &NaviTheme) {
    let items = app.get_filtered_items();
    let selected_index = app.state.selected().unwrap_or(0);
    
    let content_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    
    let title = if !app.get_filter().is_empty() {
        format!("SSL Certificates (filtered: {})", app.get_filter())
    } else {
        "SSL Certificates".to_string()
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
    const CARD_HEIGHT: u16 = 4; // 4 lines per certificate card
    let visible_cards = content_area.height / CARD_HEIGHT;
    let scroll_offset = if selected_index as u16 >= visible_cards {
        selected_index as u16 - visible_cards + 1
    } else {
        0
    };
    
    // Render individual certificate cards with scroll offset
    let mut y_offset = 0;
    for (index, cert) in items.iter().enumerate().skip(scroll_offset as usize) {
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
        
        render_cert_card(f, cert, card_area, is_selected, theme);
        y_offset += CARD_HEIGHT;
    }
    
    // Render scrollbar
    render_list_scrollbar(f, app, area, theme);
}

fn render_cert_card(f: &mut Frame, cert: &crate::tui::data::Cert, area: Rect, is_selected: bool, theme: &NaviTheme) {
    // Determine certificate status based on validity and expiration
    let cert_status = determine_cert_status(cert);
    let (status_symbol, status_style) = UiHelpers::status_indicator(cert_status, theme);
    
    // Analyze certificate security
    let is_valid = cert.is_valid() == "true" || cert.is_valid().to_lowercase() == "valid";
    let validator = CertificateValidator::default();
    let is_expiring_soon = validator.is_expiring_soon(cert.expires());
    let security_indicator = get_security_indicator(is_valid, is_expiring_soon);
    let security_style = get_security_style(is_valid, is_expiring_soon, theme);
    
    // Card background - ensure proper contrast
    let card_bg = if is_selected { theme.bg_accent } else { theme.bg_tertiary };
    let selection_indicator = if is_selected { "▶ " } else { "  " };
    
    // Parse expiration date for better display
    let expires_display = format_expiration_date(cert.expires());
    let expires_style = if is_expiring_soon {
        theme.text_style(TextType::Warning)
    } else {
        theme.text_style(TextType::Body)
    };
    
    // Create card content as multi-line text
    let content = vec![
        Line::from(vec![
            Span::raw(selection_indicator),
            Span::styled(status_symbol, status_style),
            Span::raw(" "),
            Span::styled(truncate_text(cert.host(), 30), theme.text_style(TextType::Title)),
            Span::raw("  "),
            Span::styled(security_indicator, security_style),
        ]),
        Line::from(vec![
            Span::raw("    Issuer: "),
            Span::styled(truncate_text(cert.issued_by(), 40), theme.text_style(TextType::Body)),
        ]),
        Line::from(vec![
            Span::raw("    Expires: "),
            Span::styled(expires_display, expires_style),
            Span::raw("  Valid: "),
            Span::styled(cert.is_valid(), if is_valid { 
                theme.text_style(TextType::Success) 
            } else { 
                theme.text_style(TextType::Error) 
            }),
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
    let footer_text = "v: View details • r: Renew certificate • ↑↓: Navigate • Ctrl+F: Page Down • Ctrl+B: Page Up";
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
                .title(format!("{} Filter SSL Certificates", Symbols::CHEVRON_RIGHT))
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
    
    let help_text = "ESC: Cancel • Enter: Apply • Examples: '*.example.com', 'api.*', 'expired'";
    let help = Paragraph::new(help_text)
        .style(theme.text_style(TextType::Caption).bg(theme.bg_primary))
        .alignment(Alignment::Center)
        .block(Block::default().style(Style::default().bg(theme.bg_primary)));
    
    f.render_widget(help, help_area);
}

// Helper functions

/// Determine certificate status based on validity and expiration
fn determine_cert_status(cert: &crate::tui::data::Cert) -> ResourceStatus {
    let is_valid = cert.is_valid() == "true" || cert.is_valid().to_lowercase() == "valid";
    let validator = CertificateValidator::default();
    let is_expiring_soon = validator.is_expiring_soon(cert.expires());
    
    if !is_valid {
        ResourceStatus::Failed // Invalid certificate
    } else if is_expiring_soon {
        ResourceStatus::Pending // Valid but expiring soon
    } else {
        ResourceStatus::Running // Valid and not expiring soon
    }
}

// Removed is_expired_soon function - now using CertificateValidator

/// Get security indicator emoji and text
fn get_security_indicator(is_valid: bool, is_expiring_soon: bool) -> &'static str {
    if !is_valid {
        "🔓 INVALID"
    } else if is_expiring_soon {
        "⚠️ EXPIRING"
    } else {
        "🔒 SECURE"
    }
}

/// Get appropriate text style for security status
fn get_security_style(is_valid: bool, is_expiring_soon: bool, theme: &NaviTheme) -> Style {
    if !is_valid {
        theme.text_style(TextType::Error)
    } else if is_expiring_soon {
        theme.text_style(TextType::Warning)
    } else {
        theme.text_style(TextType::Success)
    }
}

/// Format expiration date for better display
fn format_expiration_date(expires: &str) -> String {
    if expires.is_empty() || expires == "-" {
        "Unknown".to_string()
    } else if CertificateValidator::default().is_expiring_soon(expires) {
        format!("{} ⚠️", expires)
    } else {
        expires.to_string()
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