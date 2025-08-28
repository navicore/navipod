// Simplified approach for kube 1.0.0 client
use crate::error::Result as NvResult;
use kube::{Client, Config};
use std::fmt;

#[derive(Debug)]
pub struct UserAgentError {
    message: String,
}

impl fmt::Display for UserAgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for UserAgentError {}

impl UserAgentError {
    /// Creates a new `UserAgentError` with the given message.
    ///
    /// # Errors
    ///
    /// This function always returns `Ok` and never fails.
    pub fn new(message: &str) -> NvResult<Self> {
        Ok(Self {
            message: message.to_string(),
        })
    }
}

/// Create a new k8s client to interact with k8s cluster api
///
/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
pub async fn new(_custom_user_agent: Option<&str>) -> NvResult<Client> {
    // Create the Kubernetes configuration
    let config = Config::infer().await?;

    // TODO: With kube 1.0.0, the way to set a custom User-Agent has changed.
    // For now, we'll use the default User-Agent set by the client.
    // Default User-Agent: format!("kube/{kube-version} (reqwest/{reqwest-version}) {custom_agent}"
    // The custom_user_agent parameter is kept for backward compatibility but is not used.
    // When necessary, we may need to investigate a different approach with the new version.

    // Create kube client with the config
    let client = Client::try_from(config)?;

    Ok(client)
}
