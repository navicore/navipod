// Client creation with custom user-agent support for kube 2.x
use crate::error::Result as NvResult;
use hyper::http::{HeaderName, HeaderValue};
use kube::{Client, Config};
use std::env;
use tracing::{debug, warn};

/// Helper function to add user-agent header to Kubernetes config
///
/// Prioritizes in order:
/// 1. Environment variable `NAVIPOD_USER_AGENT` (if set)
/// 2. Provided `custom_user_agent` parameter
/// 3. No user-agent modification if neither is provided
pub(super) fn add_user_agent_header(config: &mut Config, custom_user_agent: Option<&str>) {
    // Check for environment variable override first
    let user_agent = env::var("NAVIPOD_USER_AGENT")
        .ok()
        .or_else(|| custom_user_agent.map(String::from));

    if let Some(ua) = user_agent {
        match HeaderValue::from_str(&ua) {
            Ok(header_value) => {
                config
                    .headers
                    .push((HeaderName::from_static("user-agent"), header_value));
                debug!("Set custom user-agent: {}", ua);
            }
            Err(e) => {
                warn!("Invalid user-agent header value '{}': {}", ua, e);
                // Fall back to default kube-rs user-agent
            }
        }
    }
}

/// Create a new k8s client to interact with k8s cluster api
///
/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
///
/// # User-Agent Configuration
///
/// The user-agent can be configured via:
/// - `NAVIPOD_USER_AGENT` environment variable (highest priority)
/// - `custom_user_agent` parameter
/// - Default: Uses kube-rs default if neither is set
pub async fn new(custom_user_agent: Option<&str>) -> NvResult<Client> {
    // Create the Kubernetes configuration
    let mut config = Config::infer().await?;

    // Add custom user-agent header using helper function
    add_user_agent_header(&mut config, custom_user_agent);

    // Create kube client with the config
    let client = Client::try_from(config)?;

    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_agent_header_valid() {
        let mut config = Config::new("https://test.example.com".parse().unwrap());

        // Test with valid user-agent
        add_user_agent_header(&mut config, Some("test-agent/1.0.0"));

        // Check that header was added
        assert_eq!(config.headers.len(), 1);
        let (name, value) = &config.headers[0];
        assert_eq!(name.as_str(), "user-agent");
        assert_eq!(value.to_str().unwrap(), "test-agent/1.0.0");
    }

    #[test]
    fn test_user_agent_header_invalid() {
        let mut config = Config::new("https://test.example.com".parse().unwrap());

        // Test with invalid user-agent (contains newline)
        add_user_agent_header(&mut config, Some("test-agent\n1.0.0"));

        // Check that no header was added due to invalid value
        assert_eq!(config.headers.len(), 0);
    }

    #[test]
    fn test_user_agent_header_none() {
        let mut config = Config::new("https://test.example.com".parse().unwrap());

        // Test with None
        add_user_agent_header(&mut config, None);

        // Check that no header was added
        assert_eq!(config.headers.len(), 0);
    }

    #[test]
    fn test_user_agent_constant() {
        // Verify the USER_AGENT constant is properly formatted
        assert!(super::super::USER_AGENT.contains("navipod/"));
        assert!(!super::super::USER_AGENT.contains('\n'));
        assert!(!super::super::USER_AGENT.contains('\r'));
    }
}
