use crate::cache_manager;
use crate::k8s::cache::config::DEFAULT_MAX_PREFETCH_REPLICASETS;
use crate::k8s::cache::DataRequest;
use crate::tui::common::app_controller::{DomainService, NavigationHandler};
use crate::tui::data::Rs;
use crate::tui::ui_loop::Apps;
use crate::tui::{event_app, ingress_app, pod_app};
use tracing::debug;

/// Domain service for `ReplicaSet` business logic
pub struct ReplicaSetDomainService {
    pub current_data: Vec<Rs>,
    pub selected_index: Option<usize>,
}

impl ReplicaSetDomainService {
    pub const fn new() -> Self {
        Self {
            current_data: Vec::new(),
            selected_index: None,
        }
    }

    pub fn get_selected_replicaset(&self) -> Option<&Rs> {
        self.selected_index
            .and_then(|index| self.current_data.get(index))
    }

    #[allow(dead_code)]
    pub const fn set_selection(&mut self, index: Option<usize>) {
        self.selected_index = index;
    }

    /// Triggers prefetch of Pod data for the given `ReplicaSets`
    /// 
    /// This function generates prefetch requests for `Pod` data based on the selectors
    /// from the provided `ReplicaSets`. It respects the configured limits for the number
    /// of `ReplicaSets` to process to avoid overwhelming the system.
    pub async fn trigger_pod_prefetch(replicasets: &[Rs], context: &str) {
        use crate::k8s::cache::fetcher::PodSelector;
        use tracing::warn;

        if let Some(bg_fetcher) = cache_manager::get_background_fetcher() {
            let namespace = cache_manager::get_current_namespace_or_default();
            let mut prefetch_requests = Vec::new();

            // Generate Pod requests for each ReplicaSet
            for rs in replicasets.iter().take(DEFAULT_MAX_PREFETCH_REPLICASETS) {
                if let Some(selectors) = &rs.selectors {
                    let pod_request = DataRequest::Pods {
                        namespace: namespace.clone(),
                        selector: PodSelector::ByLabels(selectors.clone()),
                    };
                    prefetch_requests.push(pod_request);
                }
            }

            if !prefetch_requests.is_empty() {
                debug!(
                    "🚀 {} PREFETCH: Scheduling {} Pod requests for {} ReplicaSets",
                    context,
                    prefetch_requests.len(),
                    replicasets.len()
                );
                if let Err(e) = bg_fetcher.schedule_fetch_batch(prefetch_requests).await {
                    warn!("Failed to schedule {} prefetch: {}", context, e);
                }
            }
        }
    }
}

impl Default for ReplicaSetDomainService {
    fn default() -> Self {
        Self::new()
    }
}

impl DomainService<Rs> for ReplicaSetDomainService {
    fn handle_data_update(&mut self, data: Vec<Rs>) -> Result<(), String> {
        self.current_data = data;
        // Validate selection is still valid
        if let Some(index) = self.selected_index {
            if index >= self.current_data.len() {
                self.selected_index = if self.current_data.is_empty() { 
                    None 
                } else { 
                    Some(0) 
                };
            }
        }
        Ok(())
    }

    fn get_available_actions(&self) -> Vec<String> {
        let mut actions = vec![
            "view_pods".to_string(),
            "view_yaml".to_string(),
            "refresh".to_string(),
        ];

        if self.get_selected_replicaset().is_some() {
            actions.extend_from_slice(&[
                "scale_up".to_string(),
                "scale_down".to_string(),
                "restart".to_string(),
            ]);
        }

        actions
    }

    fn execute_action(&mut self, action: &str) -> Result<Option<Apps>, String> {
        match action {
            "view_pods" => {
                self.get_selected_replicaset().map_or_else(
                    || Err("No ReplicaSet selected".into()),
                    |selected_rs| {
                        selected_rs.selectors.as_ref().map_or_else(
                            || Err("No selectors available for selected ReplicaSet".into()),
                            |selectors| {
                                Ok(Some(Apps::Pod { 
                                    app: pod_app::app::App::new(selectors.clone(), Vec::new()) 
                                }))
                            }
                        )
                    }
                )
            }
            "view_ingress" => {
                Ok(Some(Apps::Ingress { 
                    app: ingress_app::app::App::new(Vec::new()) 
                }))
            }
            "view_events" => {
                Ok(Some(Apps::Event { 
                    app: event_app::app::App::new() 
                }))
            }
            "refresh" => {
                // Trigger data refresh - this would be handled by the stream
                debug!("Refresh action triggered");
                Ok(None)
            }
            _ => {
                debug!("Unknown action: {}", action);
                Ok(None)
            }
        }
    }

    fn validate(&self) -> Result<(), String> {
        // Validate ReplicaSet data integrity
        for rs in &self.current_data {
            if rs.name.is_empty() {
                return Err("ReplicaSet with empty name found".into());
            }
            // Note: Rs struct may not have namespace field, skip this validation
            // if rs.namespace.is_empty() {
            //     return Err(format!("ReplicaSet '{}' has empty namespace", rs.name).into());
            // }
        }
        Ok(())
    }
}

/// Navigation handler for `ReplicaSet` app
pub struct ReplicaSetNavigationHandler {
    domain_service: std::cell::RefCell<ReplicaSetDomainService>,
}

impl ReplicaSetNavigationHandler {
    #[allow(dead_code)]
    pub const fn new(domain_service: ReplicaSetDomainService) -> Self {
        Self {
            domain_service: std::cell::RefCell::new(domain_service),
        }
    }
}

impl NavigationHandler for ReplicaSetNavigationHandler {
    fn handle_navigation(&self, target: &str) -> Result<Option<Apps>, String> {
        let mut service = self.domain_service.borrow_mut();
        service.execute_action(target)
    }

    fn get_navigation_targets(&self) -> Vec<String> {
        vec![
            "view_pods".to_string(),
            "view_ingress".to_string(), 
            "view_events".to_string(),
        ]
    }

    fn can_navigate_to(&self, target: &str) -> bool {
        match target {
            "view_pods" => self.domain_service.borrow().get_selected_replicaset().is_some(),
            "view_ingress" | "view_events" => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replicaset_domain_service_creation() {
        let service = ReplicaSetDomainService::new();
        assert!(service.current_data.is_empty());
        assert!(service.selected_index.is_none());
    }

    #[test]
    fn test_data_update_validation() {
        let mut service = ReplicaSetDomainService::new();
        let test_data = vec![]; // Empty data should be fine
        
        let result = service.handle_data_update(test_data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_available_actions() {
        let service = ReplicaSetDomainService::new();
        let actions = service.get_available_actions();
        
        assert!(actions.contains(&"view_pods".to_string()));
        assert!(actions.contains(&"view_yaml".to_string()));
        assert!(actions.contains(&"refresh".to_string()));
    }
}