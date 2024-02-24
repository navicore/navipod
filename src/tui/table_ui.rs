use crate::tui::data::ResourceEvent;
use crate::tui::style::{TableColors, ITEM_HEIGHT, PALETTES};
use ratatui::widgets::{ScrollbarState, TableState};
use ratatui::{prelude::*, widgets::Paragraph};

pub fn draw_timeseries_name_value_paragraphs(
    f: &mut Frame,
    background_color: Color,
    foreground_color: Color,
    area: Rect,
    event: &ResourceEvent,
    min_name_sz: u16,
) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Min(min_name_sz / 2 + 1),
                Constraint::Min(min_name_sz),
                Constraint::Percentage(90),
            ]
            .as_ref(),
        )
        .split(area); // Use the first chunk for the first name-value pair

    let age_value_paragraph = Paragraph::new(event.age.to_string())
        .style(Style::default().fg(foreground_color).bg(background_color))
        .alignment(Alignment::Right);
    f.render_widget(age_value_paragraph, layout[0]);

    let name_str = format!("{} ", event.type_);
    let name_title_paragraph = Paragraph::new(name_str)
        .style(Style::default().fg(foreground_color).bg(background_color))
        .alignment(Alignment::Right);
    f.render_widget(name_title_paragraph, layout[1]);

    let name_value_paragraph = Paragraph::new(event.message.clone())
        .style(Style::default().fg(foreground_color).bg(background_color))
        .alignment(Alignment::Left);

    f.render_widget(name_value_paragraph, layout[2]);
}

pub fn draw_name_value_paragraphs(
    f: &mut Frame,
    background_color: Color,
    foreground_color: Color,
    area: Rect,
    name: &str,
    value: &str,
    min_name_sz: u16,
) {
    let name_pair_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(min_name_sz), Constraint::Percentage(90)].as_ref())
        .split(area); // Use the first chunk for the first name-value pair

    let name_title_paragraph = Paragraph::new(name)
        .style(Style::default().fg(foreground_color).bg(background_color))
        .alignment(Alignment::Right);
    f.render_widget(name_title_paragraph, name_pair_layout[0]);

    let name_value_paragraph =
        Paragraph::new(value).style(Style::default().fg(foreground_color).bg(background_color));
    f.render_widget(name_value_paragraph, name_pair_layout[1]);
}

pub trait TuiTableState {
    type Item; // if items are of a specific type

    fn next(&mut self) {
        let pos = self.get_state().selected().unwrap_or(0);
        if pos < self.get_items().len() - 1 {
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
    fn next_color(&mut self) {
        //self.color_index = (self.color_index + 1) % PALETTES.len();
        let new_color_index = (self.get_color_index() + 1) % PALETTES.len();
        self.set_color_index(new_color_index);
    }

    fn set_colors(&mut self) {
        let new_colors = TableColors::new(&PALETTES[self.get_color_index()]);
        self.set_table_colors(new_colors);
    }

    fn get_selected_item(&mut self) -> Option<&Self::Item> {
        let selected_index = self.get_state().selected();
        let items_len = self.get_items().len();

        match selected_index {
            Some(selected) if selected < items_len => Some(&self.get_items()[selected]),
            _ => {
                self.reset_selection_state(); // Modify state as needed.
                None
            }
        }
    }

    fn get_items(&self) -> &[Self::Item];
    fn get_state(&mut self) -> &mut TableState;
    fn get_scroll_state(&self) -> &ScrollbarState;
    fn set_scroll_state(&mut self, scroll_state: ScrollbarState);
    fn get_table_colors(&self) -> &TableColors;
    fn set_table_colors(&mut self, colors: TableColors);
    fn get_color_index(&self) -> usize;
    fn set_color_index(&mut self, color_index: usize);
    fn reset_selection_state(&mut self);
    fn page_forward(&mut self);
    fn page_backward(&mut self);
    fn get_table_height(&self) -> usize;
    fn set_table_height(&mut self, table_height: usize);
}
