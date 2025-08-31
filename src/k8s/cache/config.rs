/**
 * Configuration constants for K8s cache and watch manager
 */

/// Maximum number of restart attempts for watch streams
pub const MAX_WATCH_RESTARTS: u32 = 50;

/// Maximum backoff time in seconds between restart attempts
pub const MAX_BACKOFF_SECONDS: u64 = 60;

/// Initial backoff time in seconds
pub const INITIAL_BACKOFF_SECONDS: u64 = 1;

/// Watch stream timeout in seconds (5 minutes)
pub const WATCH_TIMEOUT_SECONDS: u32 = 294;

/// Channel buffer size for invalidation events
pub const INVALIDATION_CHANNEL_CAPACITY: usize = 100;

/// Default cache size in MB
pub const DEFAULT_CACHE_SIZE_MB: usize = 100;

/// Default number of concurrent fetchers
pub const DEFAULT_CONCURRENT_FETCHERS: usize = 8;

/// Brief delay between restart attempts in seconds
pub const RESTART_DELAY_SECONDS: u64 = 1;