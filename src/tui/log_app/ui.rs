use crate::tui::log_app::app::App;
use crate::tui::table_ui::TuiTableState;
use ratatui::{
    prelude::*,
    widgets::{
        Block, Borders, Cell, Clear, HighlightSpacing, Paragraph, Row, Scrollbar,
        ScrollbarOrientation, Table,
    },
};

pub fn ui(f: &mut Frame, app: &mut App) {
    let rects = Layout::vertical([Constraint::Min(5)]).split(f.size());

    app.set_colors();

    render_table(f, app, rects[0]);

    render_scrollbar(f, app, rects[0]);

    if app.get_show_filter_edit() {
        render_filter_edit(f, app);
    }
}

fn render_filter_edit(f: &mut Frame, app: &App) {
    let edit_style = Style::default()
        .fg(app.colors.header_fg)
        .bg(app.colors.header_bg);

    let area = f.size();

    //let block = Block::default().title("Edit Filter").borders(Borders::ALL);
    let input_area = centered_rect(60, 20, area);

    let block = Paragraph::new(app.filter.as_str()).style(edit_style).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Edit Filter - try: (java|api)"),
    );

    f.render_widget(Clear, input_area); //this clears out the background
    f.render_widget(block, input_area);

    #[allow(clippy::cast_possible_truncation)]
    f.set_cursor(
        // Draw the cursor at the current position in the input field.
        // This position is can be controlled via the left and right arrow key
        input_area.x + app.edit_filter_cursor_position as u16 + 1,
        // Move one line down, from the border to the input line
        input_area.y + 1,
    );
}

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

fn render_table(f: &mut Frame, app: &mut App, area: Rect) {
    let header_style = Style::default()
        .fg(app.colors.header_fg)
        .bg(app.colors.header_bg);
    let selected_style = Style::default()
        .add_modifier(Modifier::REVERSED)
        .fg(app.colors.selected_style_fg);

    let header = ["Timestamp", "Level", "Message"]
        .iter()
        .copied()
        .map(Cell::from)
        .collect::<Row>()
        .style(header_style)
        .height(1);
    let rows = app
        .get_filtered_items()
        .into_iter()
        .enumerate()
        .map(|(i, data)| {
            let color = match i % 3 {
                0 => app.colors.normal_row_color,
                _ => app.colors.alt_row_color,
            };
            let item = data.ref_array();
            item.iter()
                .copied()
                .map(|content| Cell::from(Text::from(format!("\n{content}\n"))))
                .collect::<Row>()
                .style(Style::new().fg(app.colors.row_fg).bg(color))
                .height(3)
        });
    let bar = " █ ";
    let t = Table::new(
        rows,
        [
            // + 1 is for padding.
            Constraint::Min(app.longest_item_lens.0 + 1),
            Constraint::Min(app.longest_item_lens.1 + 1),
            Constraint::Min(app.longest_item_lens.2),
        ],
    )
    .header(header)
    .highlight_style(selected_style)
    .highlight_symbol(Text::from(vec!["".into(), bar.into(), "".into()]))
    .bg(app.colors.buffer_bg)
    .highlight_spacing(HighlightSpacing::Always);
    f.render_stateful_widget(t, area, &mut app.state);
}

fn render_scrollbar(f: &mut Frame, app: &mut App, area: Rect) {
    f.render_stateful_widget(
        Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None),
        area.inner(&Margin {
            vertical: 1,
            horizontal: 1,
        }),
        &mut app.scroll_state,
    );
}
