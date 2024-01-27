use crate::tui::style::{ITEM_HEIGHT, PALETTES, TableColors};
use ratatui::widgets::{ScrollbarState, TableState};

pub trait TuiTableState {
    type Item; // if items are of a specific type

    fn next(&mut self) {
        let i = self.get_state().selected().map_or(0, |i| {
            if i >= self.get_items().len() - 1 {
                0
            } else {
                i + 1
            }
        });

        self.get_state().select(Some(i));
        let new_scroll_state = self.get_scroll_state().position(i * ITEM_HEIGHT);
        self.set_scroll_state(new_scroll_state);
    }

    fn previous(&mut self) {
        let i = self.get_state().selected().map_or(0, |i| {
            if i == 0 {
                self.get_items().len() - 1
            } else {
                i - 1
            }
        });

        self.get_state().select(Some(i));
        let new_scroll_state = self.get_scroll_state().position(i * ITEM_HEIGHT);

        self.set_scroll_state(new_scroll_state);
    }
    fn next_color(&mut self) {
        //self.color_index = (self.color_index + 1) % PALETTES.len();
        let new_color_index = (self.get_color_index() + 1) % PALETTES.len();
        self.set_color_index(new_color_index);
    }

    fn set_colors(&mut self) {
        //self.colors = TableColors::new(&PALETTES[self.color_index]);
        let new_colors = TableColors::new(&PALETTES[self.get_color_index()]);
        self.set_table_colors(new_colors);
    }

    fn get_items(&self) -> &[Self::Item];
    fn get_state(&mut self) -> &mut TableState;
    fn get_scroll_state(&self) -> &ScrollbarState;
    fn set_scroll_state(&mut self, scroll_state: ScrollbarState);
    fn get_table_colors(&self) -> &TableColors;
    fn set_table_colors(&mut self, colors: TableColors);
    fn get_color_index(&self) -> usize;
    fn set_color_index(&mut self, color_index: usize);
}
