use crate::tui::ui_loop::Apps;

/// Trait for handling business logic operations separate from UI concerns
pub trait DomainService<T> {
    /// Handle domain-specific operations for data updates
    /// 
    /// # Errors
    /// Returns error string if data update fails validation or processing
    fn handle_data_update(&mut self, data: Vec<T>) -> Result<(), String>;
    
    /// Get domain-specific actions available for the current selection
    fn get_available_actions(&self) -> Vec<String>;
    
    /// Execute a domain action
    /// 
    /// # Errors
    /// Returns error string if action execution fails or action is invalid
    fn execute_action(&mut self, action: &str) -> Result<Option<Apps>, String>;
    
    /// Validate domain rules
    /// 
    /// # Errors
    /// Returns error string if domain validation fails
    fn validate(&self) -> Result<(), String>;
}

/// Trait for navigation logic separate from UI and domain concerns
pub trait NavigationHandler {
    /// Handle navigation to other apps based on current selection
    /// 
    /// # Errors
    /// Returns error string if navigation fails or target is invalid
    fn handle_navigation(&self, target: &str) -> Result<Option<Apps>, String>;
    
    /// Get available navigation targets from current state
    fn get_navigation_targets(&self) -> Vec<String>;
    
    /// Check if navigation target is valid
    fn can_navigate_to(&self, target: &str) -> bool;
}

/// Generic app controller that orchestrates UI, domain, and navigation concerns
pub struct AppController<T, D, N> 
where 
    D: DomainService<T>,
    N: NavigationHandler,
{
    pub domain_service: D,
    pub navigation_handler: N,
    _phantom: std::marker::PhantomData<T>,
}

impl<T, D, N> AppController<T, D, N>
where 
    D: DomainService<T>,
    N: NavigationHandler,
{
    pub const fn new(domain_service: D, navigation_handler: N) -> Self {
        Self {
            domain_service,
            navigation_handler,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Execute domain action through the service
    /// 
    /// # Errors
    /// Returns error string if domain action execution fails
    pub fn execute_domain_action(&mut self, action: &str) -> Result<Option<Apps>, String> {
        self.domain_service.execute_action(action)
    }

    /// Handle navigation through the handler  
    /// 
    /// # Errors
    /// Returns error string if navigation fails or target is invalid
    pub fn handle_navigation(&self, target: &str) -> Result<Option<Apps>, String> {
        self.navigation_handler.handle_navigation(target)
    }
}

/// Helper trait to separate UI state management from business logic
pub trait UiStateManager {
    /// Get current UI mode (normal, editing, viewing, etc.)
    fn get_ui_mode(&self) -> UiMode;
    
    /// Set UI mode
    fn set_ui_mode(&mut self, mode: UiMode);
    
    /// Check if UI is in interactive mode
    fn is_interactive(&self) -> bool;
    
    /// Get current selection context for UI rendering
    fn get_selection_context(&self) -> SelectionContext;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiMode {
    Normal,
    Editing,
    Viewing,
    FilterEditing,
    YamlViewing,
}

#[derive(Debug, Clone, Default)]
pub struct SelectionContext {
    pub selected_index: Option<usize>,
    pub total_items: usize,
    pub has_selection: bool,
    pub selection_valid: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockDomainService;
    impl DomainService<String> for MockDomainService {
        fn handle_data_update(&mut self, _data: Vec<String>) -> Result<(), String> {
            Ok(())
        }
        
        fn get_available_actions(&self) -> Vec<String> {
            vec!["test_action".to_string()]
        }
        
        fn execute_action(&mut self, _action: &str) -> Result<Option<Apps>, String> {
            Ok(None)
        }
        
        fn validate(&self) -> Result<(), String> {
            Ok(())
        }
    }

    struct MockNavigationHandler;
    impl NavigationHandler for MockNavigationHandler {
        fn handle_navigation(&self, _target: &str) -> Result<Option<Apps>, String> {
            Ok(None)
        }
        
        fn get_navigation_targets(&self) -> Vec<String> {
            vec!["test_target".to_string()]
        }
        
        fn can_navigate_to(&self, _target: &str) -> bool {
            true
        }
    }

    #[test]
    fn test_app_controller_creation() {
        let domain_service = MockDomainService;
        let navigation_handler = MockNavigationHandler;
        let _controller = AppController::new(domain_service, navigation_handler);
    }

    #[test]
    fn test_ui_mode_enum() {
        let mode = UiMode::Normal;
        assert_eq!(mode, UiMode::Normal);
    }
}