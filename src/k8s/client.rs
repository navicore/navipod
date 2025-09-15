// Client creation with custom user-agent support for kube 2.x
use crate::error::Result as NvResult;
use hyper::http::{HeaderName, HeaderValue};
use kube::{Client, Config};

/// Create a new k8s client to interact with k8s cluster api
///
/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
pub async fn new(custom_user_agent: Option<&str>) -> NvResult<Client> {
    // Create the Kubernetes configuration
    let mut config = Config::infer().await?;

    // Set custom user-agent header if provided
    // In kube 2.x, we can add custom headers directly to the Config
    // This helps identify NaviPod API calls in production environments
    if let Some(user_agent) = custom_user_agent {
        // Create a valid HeaderValue from the user agent string
        if let Ok(header_value) = HeaderValue::from_str(user_agent) {
            config.headers.push((
                HeaderName::from_static("user-agent"),
                header_value,
            ));
        }
        // If the header value is invalid, we'll just use the default user-agent
    }

    // Create kube client with the config
    let client = Client::try_from(config)?;

    Ok(client)
}
