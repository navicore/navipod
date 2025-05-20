use crate::tui::pod_app::app::App;
use crate::tui::table_ui::{TuiTableState, render_detail_section};
use ratatui::{
    prelude::*,
    widgets::{Cell, HighlightSpacing, Row, Scrollbar, ScrollbarOrientation, Table},
};

pub fn ui(f: &mut Frame, app: &mut App) {
    let rects = Layout::vertical([Constraint::Min(8), Constraint::Percentage(40)]).split(f.area());
    app.set_colors();

    let table_area = rects[0];
    let details_area = rects[1];

    render_ui_sections(f, app, table_area, details_area);
}

fn render_ui_sections(f: &mut Frame, app: &mut App, table_area: Rect, details_area: Rect) {
    render_table(f, app, table_area);
    render_scrollbar(f, app, table_area);
    render_details(f, app, details_area);
}

fn render_details(f: &mut Frame, app: &mut App, area: Rect) {
    let detail_rects =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);

    let left_details = app.get_label_details();
    let event_details = app.get_event_details();

    let (foreground_color, background_color) = get_colors(app);
    render_detail_section(
        f,
        foreground_color,
        background_color,
        detail_rects[0],
        "Label",
        &left_details,
    );
    render_detail_section(
        f,
        foreground_color,
        background_color,
        detail_rects[1],
        "Event",
        &event_details,
    );
}

const fn get_colors(app: &App) -> (Color, Color) {
    (app.colors.header_fg, app.colors.buffer_bg)
}

fn render_table(f: &mut Frame, app: &mut App, area: Rect) {
    let header_style = Style::default()
        .fg(app.colors.header_fg)
        .bg(app.colors.header_bg);
    let selected_style = Style::default()
        .add_modifier(Modifier::REVERSED)
        .fg(app.colors.selected_style_fg);

    let filter_header = match app.get_filter() {
        filter if filter != String::new() => format!("Pod ({filter})"),
        _ => "Pod".to_string(),
    };

    let header = [&filter_header, "Status", "C", "Age", "Description"]
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
            let color = match i % 2 {
                0 => app.colors.normal_row_color,
                _ => app.colors.alt_row_color,
            };
            let item = data.ref_array();
            item.iter()
                .copied()
                .map(|content| Cell::from(Text::from(format!("\n{content}\n"))))
                .collect::<Row>()
                .style(Style::new().fg(app.colors.row_fg).bg(color))
                .height(3) //height
        });
    let bar = " █ ";
    let t = Table::new(
        rows,
        [
            // + 1 is for padding.
            Constraint::Min(app.longest_item_lens.0 + 1),
            Constraint::Min(app.longest_item_lens.1 + 1),
            Constraint::Min(app.longest_item_lens.2 + 1),
            Constraint::Min(app.longest_item_lens.3 + 1),
            Constraint::Min(app.longest_item_lens.4),
        ],
    )
    .header(header)
    .row_highlight_style(selected_style)
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
        area.inner(Margin {
            vertical: 1,
            horizontal: 1,
        }),
        &mut app.scroll_state,
    );
}
