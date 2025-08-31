/**
 * Helper functions for creating common error types
 */
use crate::error::Error;
use kube::error::ErrorResponse;

/// Create an "AlreadyInitialized" error for cache components
pub fn already_initialized_error(component: &str) -> Error {
    Error::Kube(kube::Error::Api(ErrorResponse {
        status: "AlreadyExists".to_string(),
        message: format!("{component} already initialized"),
        reason: "AlreadyInitialized".to_string(),
        code: 409,
    }))
}