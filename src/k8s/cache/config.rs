/**
 * Configuration constants for K8s cache and watch manager
 */
/// Maximum number of restart attempts for watch streams
pub const MAX_WATCH_RESTARTS: u32 = 50;

/// Maximum backoff time in seconds between restart attempts
pub const MAX_BACKOFF_SECONDS: u64 = 60;

/// Initial backoff time in seconds
pub const INITIAL_BACKOFF_SECONDS: u64 = 1;

/// Watch stream timeout in seconds (294 vs 300 to allow 6 seconds for graceful shutdown)
pub const WATCH_TIMEOUT_SECONDS: u32 = 294;

/// Channel buffer size for invalidation events
pub const INVALIDATION_CHANNEL_CAPACITY: usize = 100;

/// Default cache size in MB
pub const DEFAULT_CACHE_SIZE_MB: usize = 100;

/// Default number of concurrent fetchers
pub const DEFAULT_CONCURRENT_FETCHERS: usize = 8;

/// Brief delay between restart attempts in seconds
pub const RESTART_DELAY_SECONDS: u64 = 1;

/// Validate configuration constants at compile time
const _: () = {
    assert!(MAX_WATCH_RESTARTS > 0, "MAX_WATCH_RESTARTS must be greater than 0");
    assert!(MAX_BACKOFF_SECONDS > 0, "MAX_BACKOFF_SECONDS must be greater than 0");
    assert!(INITIAL_BACKOFF_SECONDS > 0, "INITIAL_BACKOFF_SECONDS must be greater than 0");
    assert!(WATCH_TIMEOUT_SECONDS > 0, "WATCH_TIMEOUT_SECONDS must be greater than 0");
    assert!(INVALIDATION_CHANNEL_CAPACITY > 0, "INVALIDATION_CHANNEL_CAPACITY must be greater than 0");
    assert!(DEFAULT_CACHE_SIZE_MB > 0, "DEFAULT_CACHE_SIZE_MB must be greater than 0");
    assert!(DEFAULT_CONCURRENT_FETCHERS > 0, "DEFAULT_CONCURRENT_FETCHERS must be greater than 0");
    assert!(RESTART_DELAY_SECONDS > 0, "RESTART_DELAY_SECONDS must be greater than 0");
};