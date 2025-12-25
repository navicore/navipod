// Client creation with custom user-agent support for kube 2.x
use crate::error::{Error, Result as NvResult};
use hyper::http::{HeaderName, HeaderValue};
use kube::{Client, Config};
use std::env;
use tracing::{debug, warn};

/// Controls how invalid user-agent headers are handled
#[derive(Debug, Clone, Copy)]
pub enum HeaderValidationMode {
    /// Log warning and continue with default (production mode)
    Lenient,
    /// Return error on invalid headers (development/testing mode)
    Strict,
}

impl Default for HeaderValidationMode {
    fn default() -> Self {
        // Default to lenient mode for backward compatibility
        Self::Lenient
    }
}

/// Helper function to add user-agent header to Kubernetes config
///
/// Prioritizes in order:
/// 1. Environment variable `NAVIPOD_USER_AGENT` (if set)
/// 2. Provided `custom_user_agent` parameter
/// 3. No user-agent modification if neither is provided
///
/// # Errors
///
/// Returns `Err` only in strict mode when header value is invalid
pub(super) fn add_user_agent_header_with_mode(
    config: &mut Config,
    custom_user_agent: Option<&str>,
    mode: HeaderValidationMode,
) -> NvResult<()> {
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
                let msg = format!("Invalid user-agent header value '{ua}': {e}");
                match mode {
                    HeaderValidationMode::Lenient => {
                        warn!("{}", msg);
                        // Fall back to default kube-rs user-agent
                    }
                    HeaderValidationMode::Strict => {
                        return Err(Error::Custom(msg));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Helper function to add user-agent header with default lenient mode
///
/// This is a convenience wrapper that uses lenient validation mode
/// for backward compatibility
pub(super) fn add_user_agent_header(config: &mut Config, custom_user_agent: Option<&str>) {
    // Ignore the result in lenient mode as it will never error
    let _ =
        add_user_agent_header_with_mode(config, custom_user_agent, HeaderValidationMode::Lenient);
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
///
/// This function uses lenient header validation - invalid headers log warnings
/// but don't cause failures. Use `new_strict` if you need to fail on invalid headers.
pub async fn new(custom_user_agent: Option<&str>) -> NvResult<Client> {
    new_with_mode(custom_user_agent, HeaderValidationMode::Lenient).await
}

/// Create a new k8s client with strict header validation
///
/// # Errors
///
/// Will return `Err` if:
/// - Data cannot be retrieved from k8s cluster api
/// - The provided user-agent header value is invalid
///
/// Use this in development/testing to catch invalid headers early
pub async fn new_strict(custom_user_agent: Option<&str>) -> NvResult<Client> {
    new_with_mode(custom_user_agent, HeaderValidationMode::Strict).await
}

/// Internal function to create client with specified validation mode
async fn new_with_mode(
    custom_user_agent: Option<&str>,
    mode: HeaderValidationMode,
) -> NvResult<Client> {
    // Create the Kubernetes configuration
    let mut config = Config::infer().await?;

    // Add custom user-agent header using helper function with specified mode
    add_user_agent_header_with_mode(&mut config, custom_user_agent, mode)?;

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
    fn test_user_agent_header_invalid_lenient() {
        let mut config = Config::new("https://test.example.com".parse().unwrap());

        // Test with invalid user-agent (contains newline) in lenient mode
        add_user_agent_header(&mut config, Some("test-agent\n1.0.0"));

        // Check that no header was added due to invalid value
        assert_eq!(config.headers.len(), 0);
    }

    #[test]
    fn test_user_agent_header_invalid_strict() {
        let mut config = Config::new("https://test.example.com".parse().unwrap());

        // Test with invalid user-agent in strict mode
        let result = add_user_agent_header_with_mode(
            &mut config,
            Some("test-agent\n1.0.0"),
            HeaderValidationMode::Strict,
        );

        // Should return an error in strict mode
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid user-agent")
        );
    }

    #[test]
    fn test_user_agent_header_valid_strict() {
        let mut config = Config::new("https://test.example.com".parse().unwrap());

        // Test with valid user-agent in strict mode
        let result = add_user_agent_header_with_mode(
            &mut config,
            Some("test-agent/1.0.0"),
            HeaderValidationMode::Strict,
        );

        // Should succeed
        assert!(result.is_ok());
        assert_eq!(config.headers.len(), 1);
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
