pub mod cache;
pub mod client;
pub mod client_manager;
pub mod containers;
pub mod events;
pub mod metrics_client;
pub mod pod_ingress;
pub mod pods;
pub mod probes;
pub mod resources;
pub mod rs;
pub mod rs_ingress;
pub mod scan;
pub mod utils;

/// Default user agent for `NaviPod` - automatically uses the package version
///
/// ## Client Creation Pattern Guidelines:
///
/// All modules should use `client::new(Some(USER_AGENT))` for consistency.
///
/// The client module provides two modes:
/// - `client::new()` - Lenient mode (default): logs warnings for invalid headers but continues
/// - `client::new_strict()` - Strict mode: fails on invalid headers (use in tests/development)
///
/// User-agent can be overridden via `NAVIPOD_USER_AGENT` environment variable.
pub const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
