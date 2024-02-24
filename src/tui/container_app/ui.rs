use crate::tui::container_app::app::App;
use crate::tui::data::{ContainerEnvVar, ContainerMount};
use crate::tui::table_ui::{draw_name_value_paragraphs, TuiTableState};
use ratatui::{
    prelude::*,
    widgets::{
        Block, Borders, Cell, HighlightSpacing, Row, Scrollbar, ScrollbarOrientation, Table,
    },
};

pub fn ui(f: &mut Frame, app: &mut App) {
    let rects = Layout::vertical([Constraint::Min(8), Constraint::Percentage(40)]).split(f.size());

    app.set_colors();

    let table_area = rects[0];
    let rect_height = table_area.bottom() - table_area.top();

    app.set_table_height(rect_height.into());

    render_table(f, app, table_area);

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

    let details_block = create_block("Mounts (0)")
        .style(Style::default().fg(foreground_color).bg(background_color));
    f.render_widget(details_block.clone(), area);

    let mut block_title = "Mounts (0)".to_string();

    if let Some(container) = app.get_selected_item() {
        let mounts: &Vec<ContainerMount> = container.mounts.as_ref();
        let constraints = std::iter::repeat(Constraint::Length(1))
            .take(mounts.len())
            .collect::<Vec<Constraint>>();

        let num_mounts = mounts.len();
        block_title = format!("Mounts ({num_mounts})");

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints(constraints) // Pass the Vec<Constraint> as a reference
            .split(area);

        for (i, ContainerMount { name, value }) in mounts.iter().enumerate() {
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
    }

    let details_block = create_block(&block_title)
        .style(Style::default().fg(foreground_color).bg(background_color));
    f.render_widget(details_block, area);
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

    let details_block = create_block("Environment Variables (0)")
        .style(Style::default().fg(foreground_color).bg(background_color));
    f.render_widget(details_block.clone(), area);

    let mut block_title = "Environment Variables (0)".to_string();

    if let Some(container) = app.get_selected_item() {
        let envvars: &Vec<ContainerEnvVar> = container.envvars.as_ref();
        let constraints = std::iter::repeat(Constraint::Length(1))
            .take(envvars.len())
            .collect::<Vec<Constraint>>();

        let num_vars = envvars.len();
        block_title = format!("Environment Variables ({num_vars})");

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints(constraints) // Pass the Vec<Constraint> as a reference
            .split(area);

        for (i, ContainerEnvVar { name, value }) in envvars.iter().enumerate() {
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
    }

    let details_block = create_block(&block_title)
        .style(Style::default().fg(foreground_color).bg(background_color));
    f.render_widget(details_block, area);
}

// fn draw_right_details(f: &mut Frame, app: &mut App, area: Rect) {
//     let foreground_color = app.colors.header_fg;
//     let background_color = app.colors.buffer_bg;
//
//     let create_block = |title| {
//         Block::default()
//             .borders(Borders::ALL)
//             .style(Style::default().fg(foreground_color))
//             .title(Span::styled(
//                 title,
//                 Style::default().add_modifier(Modifier::BOLD),
//             ))
//     };
//
//     let mut block_title = "Environment Variables (0)".to_string();
//     let details_block = create_block(block_title.clone())
//         .style(Style::default().fg(foreground_color).bg(background_color));
//     f.render_widget(details_block, area);
//
//     if let Some(container) = app.get_selected_item() {
//     }
//
//     let details_block =
//         create_block(block_title).style(Style::default().fg(foreground_color).bg(background_color));
//     f.render_widget(details_block, area);
// }

fn render_details(f: &mut Frame, app: &mut App, area: Rect) {
    let detail_rects =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);

    draw_left_details(f, app, detail_rects[0]);
    draw_right_details(f, app, detail_rects[1]);
}

fn render_table(f: &mut Frame, app: &mut App, area: Rect) {
    let header_style = Style::default()
        .fg(app.colors.header_fg)
        .bg(app.colors.header_bg);
    let selected_style = Style::default()
        .add_modifier(Modifier::REVERSED)
        .fg(app.colors.selected_style_fg);

    let header = ["Container", "Description", "Restarts", "Image", "Ports"]
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
            .height(3) //height
    });
    let bar = " █ ";
    let t = Table::new(
        rows,
        [
            // + 1 is for padding.
            Constraint::Min(app.longest_item_lens.0 + 2),
            Constraint::Min(app.longest_item_lens.1 + 2),
            Constraint::Min(app.longest_item_lens.2 + 2),
            Constraint::Min(app.longest_item_lens.3 + 2),
            Constraint::Min(app.longest_item_lens.4 + 2),
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
