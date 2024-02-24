use ratatui::widgets::{ScrollbarState, TableState};

use crate::tui::data::{container_constraint_len_calculator, Container};
use crate::tui::style::{TableColors, ITEM_HEIGHT, PALETTES};
use crate::tui::table_ui::TuiTableState;

#[derive(Clone, Debug)]
pub struct App {
    pub(crate) state: TableState,
    pub(crate) items: Vec<Container>,
    pub(crate) longest_item_lens: (u16, u16, u16, u16, u16),
    pub(crate) scroll_state: ScrollbarState,
    pub(crate) colors: TableColors,
    color_index: usize,
    table_height: usize,
}

impl TuiTableState for App {
    type Item = Container;

    fn get_items(&self) -> &[Self::Item] {
        &self.items
    }

    fn get_state(&mut self) -> &mut TableState {
        &mut self.state
    }

    fn get_scroll_state(&self) -> &ScrollbarState {
        &self.scroll_state
    }

    fn set_scroll_state(&mut self, scroll_state: ScrollbarState) {
        self.scroll_state = scroll_state;
    }

    fn get_table_colors(&self) -> &TableColors {
        &self.colors
    }

    fn set_table_colors(&mut self, colors: TableColors) {
        self.colors = colors;
    }

    fn get_color_index(&self) -> usize {
        self.color_index
    }

    fn set_color_index(&mut self, color_index: usize) {
        self.color_index = color_index;
    }

    fn reset_selection_state(&mut self) {
        self.state = TableState::default().with_selected(0);
        self.scroll_state = ScrollbarState::new(self.items.len().saturating_sub(1) * ITEM_HEIGHT);
    }
    fn get_table_height(&self) -> usize {
        self.table_height
    }

    fn set_table_height(&mut self, table_height: usize) {
        self.table_height = table_height;
    }

    fn page_forward(&mut self) {}
    fn page_backward(&mut self) {}
}

impl App {
    pub fn new(data_vec: Vec<Container>) -> Self {
        Self {
            state: TableState::default().with_selected(0),
            longest_item_lens: container_constraint_len_calculator(&data_vec),
            scroll_state: ScrollbarState::new(data_vec.len().saturating_sub(1) * ITEM_HEIGHT),
            colors: TableColors::new(&PALETTES[0]),
            color_index: 2,
            table_height: 0,
            items: data_vec,
        }
    }
}
