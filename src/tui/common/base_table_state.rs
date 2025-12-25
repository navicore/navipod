use crate::tui::style::TableColors;
use crate::tui::yaml_editor::YamlEditor;
use ratatui::widgets::{ScrollbarState, TableState};

/// Shared state structure containing common fields used by all table-based apps
///
/// This struct consolidates the common UI state that every table-based app needs:
/// - Table navigation and selection state
/// - Scrolling and visual styling
/// - Filter editing functionality
/// - YAML viewing capability (used by apps that support 'y' key to view K8s resources)
#[derive(Clone, Debug)]
pub struct BaseTableState<T> {
    /// Ratatui table widget state for selection and navigation
    pub state: TableState,
    /// The actual data items displayed in the table
    pub items: Vec<T>,
    /// Scrollbar widget state
    pub scroll_state: ScrollbarState,
    /// Current color theme for the table
    pub colors: TableColors,
    /// Index into the available color palettes
    pub color_index: usize,
    /// Current filter text for item filtering
    pub filter: String,
    /// Whether filter editing mode is active
    pub show_filter_edit: bool,
    /// Cursor position when editing the filter
    pub edit_filter_cursor_position: usize,
    /// YAML editor for viewing Kubernetes resource definitions (supports 'y' key)
    pub yaml_editor: YamlEditor,
}

impl<T> BaseTableState<T> {
    #[must_use]
    pub fn new(items: Vec<T>) -> Self {
        Self {
            state: TableState::default().with_selected(0),
            items,
            scroll_state: ScrollbarState::new(0),
            colors: TableColors::new(&crate::tui::style::PALETTES[0]),
            color_index: 0,
            filter: String::new(),
            show_filter_edit: false,
            edit_filter_cursor_position: 0,
            yaml_editor: YamlEditor::default(),
        }
    }
}

/// Macro to generate `TuiTableState` implementation for apps using `BaseTableState`
#[macro_export]
macro_rules! impl_tui_table_state {
    ($app_type:ty, $item_type:ty) => {
        impl $crate::tui::table_ui::TuiTableState for $app_type {
            type Item = $item_type;

            fn get_items(&self) -> &[Self::Item] {
                &self.base.items
            }

            fn get_state(&mut self) -> &mut ratatui::widgets::TableState {
                &mut self.base.state
            }

            fn get_scroll_state(&self) -> &ratatui::widgets::ScrollbarState {
                &self.base.scroll_state
            }

            fn set_scroll_state(&mut self, scroll_state: ratatui::widgets::ScrollbarState) {
                self.base.scroll_state = scroll_state;
            }

            fn set_table_colors(&mut self, colors: $crate::tui::style::TableColors) {
                self.base.colors = colors;
            }

            fn get_color_index(&self) -> usize {
                self.base.color_index
            }

            fn set_color_index(&mut self, color_index: usize) {
                self.base.color_index = color_index;
            }

            fn reset_selection_state(&mut self) {
                self.base.state = ratatui::widgets::TableState::default().with_selected(0);
                self.base.scroll_state = ratatui::widgets::ScrollbarState::new(
                    self.base.items.len().saturating_sub(1) * $crate::tui::style::ITEM_HEIGHT,
                );
            }

            fn get_filter(&self) -> String {
                self.base.filter.clone()
            }

            fn set_filter(&mut self, filter: String) {
                self.base.filter = filter;
            }

            fn set_cursor_pos(&mut self, cursor_pos: usize) {
                self.base.edit_filter_cursor_position = cursor_pos;
            }

            fn get_cursor_pos(&self) -> usize {
                self.base.edit_filter_cursor_position
            }

            fn set_show_filter_edit(&mut self, show_filter_edit: bool) {
                self.base.show_filter_edit = show_filter_edit;
            }

            fn get_show_filter_edit(&self) -> bool {
                self.base.show_filter_edit
            }
        }
    };
}
