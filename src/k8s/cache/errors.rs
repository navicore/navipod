/**
 * Helper functions for creating common error types
 */
use crate::error::Error;
use kube::error::ErrorResponse;

/// Create an `AlreadyInitialized` error for cache `component`s
#[must_use]
pub fn already_initialized_error(component: &str) -> Error {
    Error::Kube(kube::Error::Api(ErrorResponse {
        status: "AlreadyExists".to_string(),
        message: format!("{component} already initialized"),
        reason: "AlreadyInitialized".to_string(),
        code: 409,
    }))
}

/// Create a `NotInitialized` error for cache components that haven't been set up
#[must_use]
pub fn cache_not_initialized_error(message: &str) -> Error {
    Error::Kube(kube::Error::Api(ErrorResponse {
        status: "FailedPrecondition".to_string(),
        message: message.to_string(),
        reason: "NotInitialized".to_string(),
        code: 412,
    }))
}

/// Create a `LockPoisoned` error for mutex/rwlock poisoning (indicates prior panic)
#[must_use]
pub fn lock_poisoned_error(message: &str) -> Error {
    Error::Kube(kube::Error::Api(ErrorResponse {
        status: "InternalError".to_string(),
        message: format!("{message} - this indicates a prior panic, application may be in an inconsistent state"),
        reason: "LockPoisoned".to_string(),
        code: 500,
    }))
}
