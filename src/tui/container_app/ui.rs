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
    let details_area = rects[1];

    render_ui_sections(f, app, table_area, details_area);
}

fn render_ui_sections(f: &mut Frame, app: &mut App, table_area: Rect, details_area: Rect) {
    set_table_height(app, table_area.height);
    render_table(f, app, table_area);
    render_scrollbar(f, app, table_area);
    render_details(f, app, details_area);
}

fn set_table_height(app: &mut App, height: u16) {
    app.set_table_height(height.into());
}

fn render_details(f: &mut Frame, app: &mut App, area: Rect) {
    let detail_rects =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);

    let mount_details = app.get_mount_details();
    let env_var_details = app.get_env_var_details();

    render_detail_section(f, app, detail_rects[0], "Mounts", &mount_details);
    render_detail_section(
        f,
        app,
        detail_rects[1],
        "Environment Variables",
        &env_var_details,
    );
}

fn render_detail_section<T: Detail>(
    f: &mut Frame,
    app: &App,
    area: Rect,
    title: &str,
    details: &Vec<T>,
) {
    let block_title = format!("{} ({})", title, details.len());
    let chunks = details
        .iter()
        .map(|d| (d.name(), d.value()))
        .collect::<Vec<_>>();

    render_block_with_title_and_details(f, app, area, &block_title, &chunks);
}

trait Detail {
    fn name(&self) -> String;
    fn value(&self) -> String;
}

impl Detail for ContainerMount {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn value(&self) -> String {
        self.value.clone()
    }
}

impl Detail for ContainerEnvVar {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn value(&self) -> String {
        self.value.clone()
    }
}

fn render_block_with_title_and_details(
    f: &mut Frame,
    app: &App,
    area: Rect,
    title: &str,
    details: &[(String, String)],
) {
    let (foreground_color, background_color) = get_colors(app);

    let create_block = |title| {
        Block::default()
            .borders(Borders::ALL)
            .style(Style::default().fg(foreground_color))
            .title(Span::styled(
                title,
                Style::default().add_modifier(Modifier::BOLD),
            ))
    };

    let details_block =
        create_block(title).style(Style::default().fg(foreground_color).bg(background_color));
    f.render_widget(details_block, area);

    let constraints = std::iter::repeat(Constraint::Length(1))
        .take(details.len())
        .collect::<Vec<Constraint>>();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(constraints) // Pass the Vec<Constraint> as a reference
        .split(area);

    for (i, (name, value)) in details.iter().enumerate() {
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

    let details_block =
        create_block(title).style(Style::default().fg(foreground_color).bg(background_color));
    f.render_widget(details_block, area);
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
