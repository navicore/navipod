use crate::tui::data::Filterable;
use crate::tui::style::{TableColors, ITEM_HEIGHT, PALETTES};
use ratatui::widgets::{Block, Borders, ScrollbarState, TableState};
use ratatui::{prelude::*, widgets::Paragraph};
use regex::Regex;
use std::rc::Rc;
use tracing::debug;

pub fn draw_timeseries_name_value_paragraphs(
    f: &mut Frame,
    background_color: Color,
    foreground_color: Color,
    area: Rect,
    name: &str,
    value: &str,
    age: &str,
) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Min(4),
                Constraint::Min(11),
                Constraint::Percentage(90),
            ]
            .as_ref(),
        )
        .split(area); // Use the first chunk for the first name-value pair

    let age_value_paragraph = Paragraph::new(age.to_string())
        .style(Style::default().fg(foreground_color).bg(background_color))
        .alignment(Alignment::Right);
    f.render_widget(age_value_paragraph, layout[0]);

    let name_str = format!("{name} ");
    let name_title_paragraph = Paragraph::new(name_str)
        .style(Style::default().fg(foreground_color).bg(background_color))
        .alignment(Alignment::Right);
    f.render_widget(name_title_paragraph, layout[1]);

    let name_value_paragraph = Paragraph::new(value)
        .style(Style::default().fg(foreground_color).bg(background_color))
        .alignment(Alignment::Left);

    f.render_widget(name_value_paragraph, layout[2]);
}

pub fn draw_name_value_paragraphs(
    f: &mut Frame,
    background_color: Color,
    foreground_color: Color,
    area: Rect,
    name: String,
    value: String,
    age: Option<String>,
) {
    if let Some(age) = age {
        draw_timeseries_name_value_paragraphs(
            f,
            background_color,
            foreground_color,
            area,
            &name,
            &value,
            &age,
        );
    } else {
        let name_pair_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(30), Constraint::Percentage(90)].as_ref())
            .split(area); // Use the first chunk for the first name-value pair

        let name_title_paragraph = Paragraph::new(name)
            .style(Style::default().fg(foreground_color).bg(background_color))
            .alignment(Alignment::Right);
        f.render_widget(name_title_paragraph, name_pair_layout[0]);

        let name_value_paragraph =
            Paragraph::new(value).style(Style::default().fg(foreground_color).bg(background_color));
        f.render_widget(name_value_paragraph, name_pair_layout[1]);
    }
}

pub trait TuiTableState
where
    Self::Item: Filterable + 'static,
{
    type Item; // if items are of a specific type

    fn next(&mut self) {
        let pos = self.get_state().selected().unwrap_or(0);
        let len = self.get_filtered_items().len();
        if len > 0 && pos < len - 1 {
            // don't wrap
            let new_pos = pos + 1;
            self.get_state().select(Some(new_pos));
            let new_scroll_state = self.get_scroll_state().position(new_pos * ITEM_HEIGHT);

            self.set_scroll_state(new_scroll_state);
        }
    }

    fn previous(&mut self) {
        let pos = self.get_state().selected().unwrap_or(0);
        if pos > 0 {
            // don't wrap
            let new_pos = pos - 1;
            self.get_state().select(Some(new_pos));
            let new_scroll_state = self.get_scroll_state().position((new_pos) * ITEM_HEIGHT);

            self.set_scroll_state(new_scroll_state);
        }
    }

    fn page_forward(&mut self) {}

    fn page_backward(&mut self) {}

    fn next_color(&mut self) {
        let new_color_index = (self.get_color_index() + 1) % PALETTES.len();
        self.set_color_index(new_color_index);
    }

    fn set_colors(&mut self) {
        let new_colors = TableColors::new(&PALETTES[self.get_color_index()]);
        self.set_table_colors(new_colors);
    }

    fn get_selected_item(&mut self) -> Option<&Self::Item> {
        let selected_index = self.get_state().selected();
        let items_len = self.get_filtered_items().len();

        match selected_index {
            Some(selected) if selected < items_len => Some(self.get_filtered_items()[selected]),
            _ => {
                self.reset_selection_state(); // Modify state as needed.
                None
            }
        }
    }

    fn get_items(&self) -> &[Self::Item];
    fn get_state(&mut self) -> &mut TableState;
    fn set_state(&mut self, state: TableState);
    fn get_scroll_state(&self) -> &ScrollbarState;
    fn set_scroll_state(&mut self, scroll_state: ScrollbarState);
    fn get_table_colors(&self) -> &TableColors;
    fn set_table_colors(&mut self, colors: TableColors);
    fn get_color_index(&self) -> usize;
    fn set_color_index(&mut self, color_index: usize);
    fn reset_selection_state(&mut self);
    fn get_table_height(&self) -> usize;
    fn set_table_height(&mut self, table_height: usize);
    fn get_filter(&self) -> String;
    fn set_filter(&mut self, filter: String);

    fn get_filtered_items(&self) -> Vec<&Self::Item> {
        let filter_pattern = self.get_filter();
        match Regex::new(&filter_pattern) {
            Ok(regex) => self
                .get_items()
                .iter()
                .filter(|item| regex.is_match(item.filter_by()))
                .collect(),
            Err(e) => {
                debug!("Invalid regex pattern: {}", e); // Log the error
                self.get_items().iter().collect() // Return the unfiltered list
            }
        }
    }
}

pub fn render_detail_section(
    f: &mut Frame,
    foreground_color: Color,
    background_color: Color,
    area: Rect,
    title: &str,
    details: &[(String, String, Option<String>)],
) {
    let block_title = format!("{} ({})", title, details.len());

    render_block_with_title_and_details(
        f,
        foreground_color,
        background_color,
        area,
        &block_title,
        details,
    );
}

fn get_chunks_from_area(area: Rect, sz: usize) -> Rc<[Rect]> {
    let constraints = std::iter::repeat(Constraint::Length(1))
        .take(sz)
        .collect::<Vec<Constraint>>();

    Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(constraints) // Pass the Vec<Constraint> as a reference
        .split(area)
}

fn create_block(title: String, foreground_color: &Color) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .style(Style::default().fg(*foreground_color))
        .title(Span::styled(
            title,
            Style::default().add_modifier(Modifier::BOLD),
        ))
}

fn render_block_with_title_and_details(
    f: &mut Frame,
    foreground_color: Color,
    background_color: Color,
    area: Rect,
    title: &str,
    details: &[(String, String, Option<String>)],
) {
    let details_block = create_block(title.to_string(), &foreground_color)
        .style(Style::default().fg(foreground_color).bg(background_color));
    f.render_widget(details_block, area);

    let chunks = get_chunks_from_area(area, details.len());

    for (i, (name, value, age)) in details.iter().enumerate() {
        let formatted_name = format!("{}: ", &name);
        if let Some(chunk) = chunks.get(i) {
            draw_name_value_paragraphs(
                f,
                background_color,
                foreground_color,
                *chunk,
                formatted_name.to_string(),
                value.to_string(),
                age.clone(),
            );
        }
    }

    let details_block = create_block(title.to_string(), &foreground_color)
        .style(Style::default().fg(foreground_color).bg(background_color));
    f.render_widget(details_block, area);
}
