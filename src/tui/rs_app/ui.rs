use crate::tui::rs_app::app::App;
use crate::tui::table_ui::{draw_name_value_paragraphs, TuiTableState};
use k8s_openapi::api::core::v1::Event;
use ratatui::{
    prelude::*,
    widgets::{
        Block, Borders, Cell, HighlightSpacing, Row, Scrollbar, ScrollbarOrientation, Table,
    },
};

pub fn ui(f: &mut Frame, app: &mut App) {
    let rects = Layout::vertical([Constraint::Min(8), Constraint::Percentage(40)]).split(f.size());

    app.set_colors();

    render_table(f, app, rects[0]);

    render_scrollbar(f, app, rects[0]);

    render_details(f, app, rects[1]);
}

fn draw_left_details(f: &mut Frame, app: &mut App, area: Rect) {
    let foreground_color = app.colors.header_fg;
    let background_color = app.colors.buffer_bg;

    let create_block = |title| {
        Block::default()
            .borders(Borders::ALL)
            .style(Style::default().fg(foreground_color))
            .title(Span::styled(
                title,
                Style::default().add_modifier(Modifier::BOLD),
            ))
    };

    let mut block_title = "Labels".to_string();

    if let Some(rs) = app.get_selected_item() {
        if let Some(labels) = rs.selectors.as_ref() {
            let num_labels = labels.len();
            block_title = format!("Labels ({num_labels})");
            let constraints = std::iter::repeat(Constraint::Length(1))
                .take(labels.len())
                .collect::<Vec<Constraint>>();

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(constraints) // Pass the Vec<Constraint> as a reference
                .split(area);

            for (i, (name, value)) in labels.iter().enumerate() {
                let formatted_name = format!("{}: ", &name);
                if let Some(chunk) = chunks.get(i) {
                    draw_name_value_paragraphs(
                        f,
                        background_color,
                        foreground_color,
                        *chunk,
                        &formatted_name,
                        value,
                        30,
                    );
                }
            }
        };

        let details_block = create_block(block_title)
            .style(Style::default().fg(foreground_color).bg(background_color));
        f.render_widget(details_block, area);
    }
}

fn draw_right_details(f: &mut Frame, app: &mut App, area: Rect) {
    let foreground_color = app.colors.header_fg;
    let background_color = app.colors.buffer_bg;

    let create_block = |title| {
        Block::default()
            .borders(Borders::ALL)
            .style(Style::default().fg(foreground_color))
            .title(Span::styled(
                title,
                Style::default().add_modifier(Modifier::BOLD),
            ))
    };

    let mut block_title = "Events".to_string();
    if let Some(rs) = app.get_selected_item() {
        let events: &Vec<Event> = rs.events.as_ref();
        let num_events = events.len();
        block_title = format!("Events ({num_events})");

        let mut sorted_events = events.clone();

        sorted_events.sort_by(|a, b| {
            b.last_timestamp
                .clone()
                .map_or_else(chrono::Utc::now, |t| t.0)
                .cmp(
                    &a.last_timestamp
                        .clone()
                        .map_or_else(chrono::Utc::now, |t| t.0),
                )
        });

        let event_display_height = 1; // Adjust based on your actual layout
        let max_events = area.height as usize / event_display_height - 1;

        let recent_events = sorted_events.iter().take(max_events).collect::<Vec<_>>();

        for (i, event) in recent_events.iter().enumerate() {
            let pos = i + 1;
            let formatted_name = format!("{}: ", event.type_.as_ref().unwrap_or(&String::new()));
            let temp = "empty".to_string();
            let value = event.message.as_ref().unwrap_or(&temp);
            #[allow(clippy::cast_possible_truncation)]
            let chunk = Rect {
                x: area.x,
                y: area.y + pos as u16 * event_display_height as u16,
                width: area.width,
                height: 1,
            };
            draw_name_value_paragraphs(
                f,
                background_color,
                foreground_color,
                chunk,
                &formatted_name,
                value,
                10,
            );
        }
    }

    let details_block =
        create_block(block_title).style(Style::default().fg(foreground_color).bg(background_color));
    f.render_widget(details_block, area);
}

fn render_details(f: &mut Frame, app: &mut App, area: Rect) {
    let detail_rects =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);

    draw_left_details(f, app, detail_rects[0]);
    draw_right_details(f, app, detail_rects[1]);

    //f.render_widget(paragraph, area);
}

fn render_table(f: &mut Frame, app: &mut App, area: Rect) {
    let header_style = Style::default()
        .fg(app.colors.header_fg)
        .bg(app.colors.header_bg);
    let selected_style = Style::default()
        .add_modifier(Modifier::REVERSED)
        .fg(app.colors.selected_style_fg);

    let header = ["ReplicaSet", "P", "Age", "Description", "Owner"]
        .iter()
        .copied()
        .map(Cell::from)
        .collect::<Row>()
        .style(header_style)
        .height(1);
    let rows = app.items.iter().enumerate().map(|(i, data)| {
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
            .height(3) // height
    });
    let bar = " █ ";
    let t = Table::new(
        rows,
        [
            // + 1 is for padding.
            Constraint::Length(app.longest_item_lens.0 + 1),
            Constraint::Min(app.longest_item_lens.1 + 1),
            Constraint::Min(app.longest_item_lens.2 + 1),
            Constraint::Min(app.longest_item_lens.3 + 1),
            Constraint::Min(app.longest_item_lens.4 + 1),
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
