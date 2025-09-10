use crate::tui::table_ui::TuiTableState;
use crate::tui::ui_loop::Apps;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Result of handling a common key event
#[derive(Debug, Clone)]
pub enum KeyHandlerResult {
    /// Key was handled, return the specified app
    Handled(Option<Apps>),
    /// Key was handled, but app should be updated with the provided instance
    HandledWithUpdate(Option<Apps>),
    /// Key was not handled, app should handle it
    NotHandled,
    /// Request to quit the application
    Quit,
}

/// Handles common key events that are shared across all table-based apps
/// Returns `KeyHandlerResult` indicating whether the key was handled and what action to take
pub fn handle_common_keys<T: TuiTableState + Clone>(
    app: &mut T,
    key_event: &KeyEvent,
    app_variant: impl Fn(T) -> Apps,
) -> KeyHandlerResult {
    match key_event.code {
        // Quit application
        KeyCode::Char('q') | KeyCode::Esc => KeyHandlerResult::Quit,

        // Navigation: j/k (up/down)
        KeyCode::Char('j') | KeyCode::Down => {
            app.next();
            KeyHandlerResult::HandledWithUpdate(Some(app_variant(app.clone())))
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.previous();
            KeyHandlerResult::HandledWithUpdate(Some(app_variant(app.clone())))
        }

        // Color cycling
        KeyCode::Char('c' | 'C') => {
            app.next_color();
            KeyHandlerResult::HandledWithUpdate(Some(app_variant(app.clone())))
        }

        // Page forward/backward
        KeyCode::Char('f' | 'F') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            app.page_forward();
            KeyHandlerResult::HandledWithUpdate(Some(app_variant(app.clone())))
        }
        KeyCode::Char('b' | 'B') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            app.page_backward();
            KeyHandlerResult::HandledWithUpdate(Some(app_variant(app.clone())))
        }

        // Vim-style navigation
        KeyCode::Char('G') => {
            app.jump_to_bottom();
            KeyHandlerResult::HandledWithUpdate(Some(app_variant(app.clone())))
        }
        KeyCode::Char('g') => {
            app.jump_to_top();
            KeyHandlerResult::HandledWithUpdate(Some(app_variant(app.clone())))
        }

        // Not a common key
        _ => KeyHandlerResult::NotHandled,
    }
}

/// Handles YAML editor key events
pub fn handle_yaml_editor_keys(
    key_event: &KeyEvent,
    yaml_editor: &mut crate::tui::yaml_editor::YamlEditor,
) -> bool {
    match key_event.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            yaml_editor.close();
            true
        }
        KeyCode::Char('r' | 'R') => {
            let _ = yaml_editor.fetch_yaml(); // Ignore errors for now
            true
        }
        KeyCode::Up | KeyCode::Char('k') => {
            yaml_editor.scroll_up(3);
            true
        }
        KeyCode::Down | KeyCode::Char('j') => {
            yaml_editor.scroll_down(3, None);
            true
        }
        KeyCode::Char('G') => {
            yaml_editor.jump_to_bottom(None);
            true
        }
        KeyCode::Char('g') => {
            yaml_editor.jump_to_top();
            true
        }
        _ => false, // Key not handled
    }
}

/// Helper function to activate filter editing mode
pub fn handle_filter_activation<T: TuiTableState + Clone>(
    app: &mut T,
    key_event: &KeyEvent,
    app_variant: impl Fn(T) -> Apps,
) -> KeyHandlerResult {
    match key_event.code {
        KeyCode::Char('/') => {
            app.set_show_filter_edit(true);
            KeyHandlerResult::HandledWithUpdate(Some(app_variant(app.clone())))
        }
        _ => KeyHandlerResult::NotHandled,
    }
}

/// Handles filter editing key events (for apps that support filtering)
/// Returns true if the key was handled, false otherwise
pub fn handle_filter_editing_keys<T: TuiTableState + Clone>(
    app: &mut T,
    key_event: &KeyEvent,
    app_variant: impl Fn(T) -> Apps,
) -> Option<Apps> {
    match key_event.code {
        KeyCode::Char(to_insert) => {
            app.enter_char(to_insert);
            Some(app_variant(app.clone()))
        }
        KeyCode::Backspace => {
            app.delete_char();
            Some(app_variant(app.clone()))
        }
        KeyCode::Left => {
            app.move_cursor_left();
            Some(app_variant(app.clone()))
        }
        KeyCode::Right => {
            app.move_cursor_right();
            Some(app_variant(app.clone()))
        }
        KeyCode::Esc | KeyCode::Enter => {
            app.set_show_filter_edit(false);
            Some(app_variant(app.clone()))
        }
        _ => None, // Key not handled
    }
}

#[cfg(test)]
mod tests {

    // Basic test structure - would need to be expanded with actual test implementations
    #[test]
    fn test_quit_key_handling() {
        // Test that 'q' and Esc keys return Quit result
    }

    #[test]
    fn test_navigation_key_handling() {
        // Test that G/g keys trigger jump operations
    }

    #[test]
    fn test_color_cycling() {
        // Test that c/C keys trigger color changes
    }
}
