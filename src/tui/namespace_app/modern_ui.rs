use crate::tui::namespace_app::app::App;
use crate::tui::table_ui::TuiTableState;
use crate::tui::theme::{NaviTheme, ResourceStatus, Symbols, TextType, UiConstants, UiHelpers};
use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Clear, Paragraph, Scrollbar,
    ScrollbarOrientation, Wrap
};

/// Modern card-based UI for Namespace picker
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
        Constraint::Length(UiConstants::ICON_COLUMN_WIDTH),  // Icon + Title
        Constraint::Min(0),      // Context info (flexible)
        Constraint::Length(UiConstants::ACTIONS_COLUMN_WIDTH),  // Actions
    ]).split(area);

    // Title with icon
    let title_text = format!("{} Select Namespace", Symbols::NAMESPACE);
    let title = Paragraph::new(title_text)
        .style(theme.text_style(TextType::Title).bg(theme.bg_primary))
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(title, header_chunks[0]);

    // Context info - also show filtered count for debugging
    let namespaces = app.get_items();
    let total_count = namespaces.len();
    let filtered_count = app.get_filtered_items().len();
    let current = namespaces.iter().find(|ns| ns.is_current);
    let current_name = current.map_or("none", |ns| ns.name.as_str());

    let context_text = format!("{total_count} namespaces ({filtered_count} shown) • current: {current_name}");

    let context = Paragraph::new(context_text)
        .style(theme.text_style(TextType::Caption).bg(theme.bg_primary))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(context, header_chunks[1]);

    // Actions/shortcuts
    let actions_text = "/: filter • Enter: select • Esc: cancel • c: colors • q: quit";
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
    render_namespace_list(f, app, area, theme);
}

fn render_namespace_list(f: &mut Frame, app: &App, area: Rect, theme: &NaviTheme) {
    let items = app.get_filtered_items();
    let selected_index = app.base.state.selected().unwrap_or(0);

    let content_area = area.inner(Margin { vertical: 1, horizontal: 1 });

    let filter = app.get_filter();
    let title = if filter.is_empty() {
        "Namespaces".to_string()
    } else {
        format!("Namespaces (filtered: {filter})")
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
    let scroll_offset = if UiHelpers::safe_cast_u16(selected_index, "namespace scroll offset") >= visible_cards {
        UiHelpers::safe_cast_u16(selected_index, "namespace scroll offset") - visible_cards + 1
    } else {
        0
    };

    // Render individual namespace cards with scroll offset
    let mut y_offset = 0;
    for (index, namespace) in items.iter().enumerate().skip(scroll_offset as usize) {
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

        render_namespace_card(f, namespace, card_area, is_selected, theme);
        y_offset += UiConstants::CARD_HEIGHT;
    }

    // Render scrollbar
    render_list_scrollbar(f, app, area, theme);
}

fn render_namespace_card(f: &mut Frame, namespace: &crate::tui::data::Namespace, area: Rect, is_selected: bool, theme: &NaviTheme) {
    // Determine namespace status
    let ns_status = determine_namespace_status(namespace);
    let (status_symbol, status_style) = UiHelpers::status_indicator(ns_status, theme);

    // Card background - ensure proper contrast
    let card_bg = if is_selected { theme.bg_accent } else { theme.bg_tertiary };
    let selection_indicator = if is_selected { "▶ " } else { "  " };

    // Current namespace indicator
    let current_indicator = if namespace.is_current {
        Span::styled(" (current)", theme.text_style(TextType::Success))
    } else {
        Span::raw("")
    };

    // Create card content as multi-line text
    let content = vec![
        Line::from(vec![
            Span::raw(selection_indicator),
            Span::styled(status_symbol, status_style),
            Span::raw(" "),
            Span::styled(&namespace.name, if namespace.is_current {
                theme.text_style(TextType::Title).add_modifier(Modifier::BOLD)
            } else {
                theme.text_style(TextType::Title)
            }),
            current_indicator,
        ]),
        Line::from(vec![
            Span::raw("    Status: "),
            Span::styled(&namespace.status, status_style),
            Span::raw("  •  Age: "),
            Span::styled(&namespace.age, theme.text_style(TextType::Caption)),
        ]),
        Line::from(vec![
            // Add spacing line for card separation
            Span::raw(""),
        ]),
        Line::from(vec![
            Span::raw(""),
        ]),
    ];

    let card = Paragraph::new(content)
        .style(Style::default().bg(card_bg));

    f.render_widget(card, area);
}

fn render_list_scrollbar(f: &mut Frame, app: &App, area: Rect, theme: &NaviTheme) {
    let items = app.get_filtered_items();
    let content_area = area.inner(Margin { vertical: 1, horizontal: 1 });
    let visible_cards = content_area.height / UiConstants::CARD_HEIGHT;

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
    let footer_text = "Enter: Switch to namespace • Esc: Cancel • ↑↓: Navigate • G: Bottom • g: Top • Ctrl+F/B: Page";
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
                .title(format!("{} Filter Namespaces", Symbols::CHEVRON_RIGHT))
                .title_style(theme.text_style(TextType::Subtitle).bg(theme.bg_secondary))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_focus).bg(theme.bg_secondary).add_modifier(Modifier::BOLD))
                .style(Style::default().bg(theme.bg_secondary))
        )
        .wrap(Wrap { trim: true });

    f.render_widget(filter_input, modal_area);

    // Set cursor position
    let cursor_pos = Position {
        x: modal_area.x + UiHelpers::safe_cast_u16(app.get_cursor_pos(), "namespace cursor position") + 1,
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

    let help_text = "ESC: Cancel • Enter: Apply • Examples: 'prod*', '.*dev.*', 'kube-*'";
    let help = Paragraph::new(help_text)
        .style(theme.text_style(TextType::Caption).bg(theme.bg_primary))
        .alignment(Alignment::Center)
        .block(Block::default().style(Style::default().bg(theme.bg_primary)));

    f.render_widget(help, help_area);
}

// Helper functions

/// Determine namespace status based on phase
fn determine_namespace_status(namespace: &crate::tui::data::Namespace) -> ResourceStatus {
    match namespace.status.as_str() {
        "Active" => ResourceStatus::Running,
        "Terminating" => ResourceStatus::Pending,
        _ => ResourceStatus::Unknown,
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
