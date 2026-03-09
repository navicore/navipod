/**
 * Helper functions for creating common error types
 */
use crate::error::Error;
use kube::core::Status;

/// Create an `AlreadyInitialized` error for cache `component`s
#[must_use]
pub fn already_initialized_error(component: &str) -> Error {
    Error::Kube(kube::Error::Api(
        Status::failure(
            &format!("{component} already initialized"),
            "AlreadyInitialized",
        )
        .with_code(409)
        .boxed(),
    ))
}

/// Create a `NotInitialized` error for cache components that haven't been set up
#[must_use]
pub fn cache_not_initialized_error(message: &str) -> Error {
    Error::Kube(kube::Error::Api(
        Status::failure(message, "NotInitialized")
            .with_code(412)
            .boxed(),
    ))
}

/// Create a `LockPoisoned` error for mutex/rwlock poisoning (indicates prior panic)
#[must_use]
pub fn lock_poisoned_error(message: &str) -> Error {
    Error::Kube(kube::Error::Api(
        Status::failure(
            &format!(
                "{message} - this indicates a prior panic, application may be in an inconsistent state"
            ),
            "LockPoisoned",
        )
        .with_code(500)
        .boxed(),
    ))
}
