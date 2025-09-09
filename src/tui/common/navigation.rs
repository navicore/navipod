use crate::tui::table_ui::TuiTableState;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Standard vim-style navigation handler for table-based apps
pub fn handle_vim_navigation<T: TuiTableState>(app: &mut T, key_event: &KeyEvent) -> bool {
    match key_event.code {
        KeyCode::Char('j') | KeyCode::Down => {
            app.next();
            true
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.previous();
            true
        }
        KeyCode::Char('G') => {
            app.jump_to_bottom();
            true
        }
        KeyCode::Char('g') => {
            app.jump_to_top();
            true
        }
        KeyCode::Char('f' | 'F') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            app.page_forward();
            true
        }
        KeyCode::Char('b' | 'B') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            app.page_backward();
            true
        }
        _ => false,
    }
}

/// Standard color cycling handler
pub fn handle_color_cycling<T: TuiTableState>(app: &mut T, key_event: &KeyEvent) -> bool {
    match key_event.code {
        KeyCode::Char('c' | 'C') => {
            app.next_color();
            true
        }
        _ => false,
    }
}
