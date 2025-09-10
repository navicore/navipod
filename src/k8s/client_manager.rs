use kube::{Client, Config};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error};
use crate::error::Result as NvResult;

/// Singleton Kubernetes client manager that handles client lifecycle,
/// credential caching, and token refresh automatically.
pub struct K8sClientManager {
    client: RwLock<Option<Arc<Client>>>,
    config: RwLock<Option<Config>>,
}

impl K8sClientManager {
    /// Create a new client manager instance
    const fn new() -> Self {
        Self {
            client: RwLock::const_new(None),
            config: RwLock::const_new(None),
        }
    }
    
    /// Get or create a Kubernetes client. This method handles:
    /// - Lazy initialization on first call
    /// - Caching of client instance to avoid repeated credential reads
    /// - Automatic token refresh when credentials expire
    /// - Thread-safe access in multiprocessing environments
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Kubernetes configuration cannot be inferred
    /// - Client creation fails
    /// - Credentials are invalid and cannot be refreshed
    pub async fn get_client(&self) -> NvResult<Arc<Client>> {
        // First, try to get existing client
        {
            let client_guard = self.client.read().await;
            if let Some(ref client) = *client_guard {
                debug!("♻️ Reusing existing Kubernetes client");
                return Ok(client.clone());
            }
        }
        
        // No client exists, need to create one
        debug!("🔧 Creating new Kubernetes client");
        self.create_client().await
    }
    
    /// Force refresh the client (e.g., when auth errors occur)
    ///
    /// # Errors
    ///
    /// Returns an error if client creation fails
    pub async fn refresh_client(&self) -> NvResult<Arc<Client>> {
        debug!("🔄 Force refreshing Kubernetes client due to auth error");
        
        // Clear existing client and config
        {
            let mut client_guard = self.client.write().await;
            *client_guard = None;
        }
        {
            let mut config_guard = self.config.write().await;
            *config_guard = None;
        }
        
        // Create new client
        self.create_client().await
    }
    
    /// Internal method to create a new client
    async fn create_client(&self) -> NvResult<Arc<Client>> {
        // Get or create config
        let config = {
            let config_guard = self.config.read().await;
            if let Some(ref config) = *config_guard {
                config.clone()
            } else {
                // Need to create config
                drop(config_guard); // Release read lock
                
                debug!("📄 Loading Kubernetes configuration from default sources");
                let new_config = Config::infer().await.map_err(|e| {
                    error!("❌ Failed to infer Kubernetes configuration: {}", e);
                    e
                })?;
                
                // Store the config for reuse
                {
                    let mut config_guard = self.config.write().await;
                    *config_guard = Some(new_config.clone());
                }
                
                new_config
            }
        };
        
        // Create client from config
        let client = Client::try_from(config).map_err(|e| {
            error!("❌ Failed to create Kubernetes client: {}", e);
            e
        })?;
        
        let client_arc = Arc::new(client);
        
        // Store the client
        {
            let mut client_guard = self.client.write().await;
            *client_guard = Some(client_arc.clone());
        }
        
        debug!("✅ Successfully created new Kubernetes client");
        Ok(client_arc)
    }
    
    /// Check if we have a cached client
    pub async fn has_client(&self) -> bool {
        let client_guard = self.client.read().await;
        client_guard.is_some()
    }
}

/// Global singleton instance of the client manager
static CLIENT_MANAGER: K8sClientManager = K8sClientManager::new();

/// Get the global Kubernetes client. This is the main entry point for all
/// Kubernetes API operations. It handles caching, token refresh, and
/// thread-safe access automatically.
///
/// # Errors
///
/// Returns an error if the client cannot be created or refreshed
pub async fn get_client() -> NvResult<Arc<Client>> {
    CLIENT_MANAGER.get_client().await
}

/// Force refresh the global Kubernetes client. This should be called when
/// authentication errors occur to get a fresh client with updated credentials.
///
/// # Errors
///
/// Returns an error if the client cannot be refreshed
pub async fn refresh_client() -> NvResult<Arc<Client>> {
    CLIENT_MANAGER.refresh_client().await
}

/// Check if we have a cached client without creating one
pub async fn has_cached_client() -> bool {
    CLIENT_MANAGER.has_client().await
}

/// Helper function for handling Kubernetes API errors that might indicate
/// authentication issues. Returns true if the client should be refreshed.
pub const fn should_refresh_client(error: &kube::Error) -> bool {
    match error {
        kube::Error::Api(api_error) => {
            // Check for authentication/authorization errors
            matches!(api_error.code, 401 | 403)
        }
        kube::Error::Auth(_) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_client_manager_singleton() {
        // Test that multiple calls return the same client instance
        if let (Ok(client1), Ok(client2)) = (get_client().await, get_client().await) {
            // Should be the same Arc instance
            assert!(Arc::ptr_eq(&client1, &client2));
        }
    }
    
    #[tokio::test]
    async fn test_client_refresh() {
        // Test that refresh creates a new client
        if let (Ok(client1), Ok(client2)) = (get_client().await, refresh_client().await) {
            // Should be different Arc instances after refresh
            assert!(!Arc::ptr_eq(&client1, &client2));
        }
    }
    
    #[test]
    fn test_should_refresh_client() {
        use kube::error::ErrorResponse;
        
        // Test 401 error should trigger refresh
        let auth_error = kube::Error::Api(ErrorResponse {
            status: "Failure".to_string(),
            message: "Unauthorized".to_string(),
            reason: "Unauthorized".to_string(),
            code: 401,
        });
        assert!(should_refresh_client(&auth_error));
        
        // Test 403 error should trigger refresh
        let forbidden_error = kube::Error::Api(ErrorResponse {
            status: "Failure".to_string(),
            message: "Forbidden".to_string(),
            reason: "Forbidden".to_string(),
            code: 403,
        });
        assert!(should_refresh_client(&forbidden_error));
        
        // Test other errors should not trigger refresh
        let not_found_error = kube::Error::Api(ErrorResponse {
            status: "Failure".to_string(),
            message: "Not Found".to_string(),
            reason: "NotFound".to_string(),
            code: 404,
        });
        assert!(!should_refresh_client(&not_found_error));
    }
}