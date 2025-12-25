/**
Global cache manager for `NaviPod`

This module provides a singleton cache that can be accessed throughout the app.
It initializes the cache and background fetcher at startup.
*/
use crate::error::Result;
use crate::k8s::cache::{
    BackgroundFetcher, DataRequest, FetchResult, K8sDataCache, WatchManager, WatchManagerHandle,
    config::{DEFAULT_CACHE_SIZE_MB, DEFAULT_CONCURRENT_FETCHERS},
    errors::already_initialized_error,
};
use crate::k8s::metrics_history::MetricsHistoryStore;
use std::sync::{Arc, OnceLock, RwLock};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Global cache instance
static CACHE: OnceLock<Arc<K8sDataCache>> = OnceLock::new();
/// Background fetcher instance
static BACKGROUND_FETCHER: OnceLock<Arc<BackgroundFetcher>> = OnceLock::new();
/// Background fetcher shutdown channel
static FETCHER_SHUTDOWN_TX: OnceLock<mpsc::Sender<()>> = OnceLock::new();

/// Mutable namespace state (namespace + watcher state that changes on namespace switch)
struct NamespaceState {
    namespace: String,
    watcher_shutdown_tx: mpsc::Sender<()>,
    watcher_handle: WatchManagerHandle,
}

/// Namespace and watcher state (mutable via RwLock for namespace switching)
/// Using std::sync::RwLock since we need sync access from various contexts
static NAMESPACE_STATE: OnceLock<std::sync::RwLock<NamespaceState>> = OnceLock::new();
/// Global metrics history store for trend visualization
static METRICS_HISTORY: OnceLock<Arc<RwLock<MetricsHistoryStore>>> = OnceLock::new();
/// Metrics history cleanup task shutdown channel
static METRICS_CLEANUP_SHUTDOWN_TX: OnceLock<mpsc::Sender<()>> = OnceLock::new();
/// Global counter for active network operations (blocking IO)
static NETWORK_ACTIVITY_COUNTER: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
/// Global counter for blocking network operations (cache misses - should be red!)
static BLOCKING_ACTIVITY_COUNTER: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
/// Last time we had blocking activity (for minimum display time)
static LAST_BLOCKING_TIME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Last time we had network activity (for minimum display time)  
static LAST_NETWORK_TIME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Initialize the global cache and background fetcher
///
/// # Errors
///
/// Returns an error if cache is already initialized or if initialization fails
#[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
pub async fn initialize_cache(namespace: String) -> Result<()> {
    let cache = Arc::new(K8sDataCache::new(DEFAULT_CACHE_SIZE_MB));
    let fetcher = BackgroundFetcher::new(cache.clone(), DEFAULT_CONCURRENT_FETCHERS);

    let (fetcher_arc, fetcher_shutdown_tx) = fetcher.start();

    // Initialize watch manager for real-time invalidation (namespace-scoped)
    let watch_manager = match WatchManager::new(cache.clone(), namespace.clone()).await {
        Ok(wm) => wm,
        Err(e) => {
            // Cleanup fetcher if watch manager initialization fails
            let _ = fetcher_shutdown_tx.send(()).await;
            return Err(e);
        }
    };
    let (watcher_shutdown_tx, watcher_handle) = watch_manager.start();

    // Initialize metrics history store
    let metrics_history = Arc::new(RwLock::new(MetricsHistoryStore::new()));
    if METRICS_HISTORY.set(metrics_history.clone()).is_err() {
        error!("Metrics history already initialized");
        let _ = fetcher_shutdown_tx.send(()).await;
        let _ = watcher_shutdown_tx.send(()).await;
        return Err(already_initialized_error("Metrics history"));
    }

    // Start periodic cleanup task for metrics history
    let (cleanup_shutdown_tx, mut cleanup_shutdown_rx) = mpsc::channel::<()>(1);
    let metrics_history_clone = metrics_history.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(600)); // 10 minutes
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Ok(mut store) = metrics_history_clone.write() {
                        store.prune_all();
                        info!("Metrics history cleaned up (removed empty entries)");
                    } else {
                        warn!("Failed to acquire write lock for metrics history cleanup");
                    }
                }
                _ = cleanup_shutdown_rx.recv() => {
                    info!("Metrics history cleanup task shutting down");
                    break;
                }
            }
        }
    });

    if METRICS_CLEANUP_SHUTDOWN_TX
        .set(cleanup_shutdown_tx.clone())
        .is_err()
    {
        error!("Metrics cleanup shutdown channel already initialized");
        let _ = cleanup_shutdown_tx.send(()).await; // Shutdown the spawned task
        let _ = fetcher_shutdown_tx.send(()).await;
        let _ = watcher_shutdown_tx.send(()).await;
        return Err(already_initialized_error(
            "Metrics cleanup shutdown channel",
        ));
    }

    // Store all global state atomically to prevent race conditions
    // If any step fails, we need to clean up properly

    if CACHE.set(cache.clone()).is_err() {
        error!("Cache already initialized");
        let _ = fetcher_shutdown_tx.send(()).await;
        let _ = watcher_shutdown_tx.send(()).await;
        if let Some(cleanup_tx) = METRICS_CLEANUP_SHUTDOWN_TX.get() {
            let _ = cleanup_tx.send(()).await;
        }
        return Err(already_initialized_error("Cache"));
    }

    // Store background fetcher BEFORE storing shutdown channels to ensure
    // get_background_fetcher() returns valid instance
    if BACKGROUND_FETCHER.set(fetcher_arc).is_err() {
        error!("Background fetcher already initialized");
        let _ = fetcher_shutdown_tx.send(()).await;
        let _ = watcher_shutdown_tx.send(()).await;
        if let Some(cleanup_tx) = METRICS_CLEANUP_SHUTDOWN_TX.get() {
            let _ = cleanup_tx.send(()).await;
        }
        return Err(already_initialized_error("Background fetcher"));
    }

    if FETCHER_SHUTDOWN_TX.set(fetcher_shutdown_tx).is_err() {
        error!("Fetcher shutdown channel already initialized");
        return Err(already_initialized_error("Fetcher shutdown channel"));
    }

    // Store namespace state (namespace + watcher) in a single RwLock for atomic updates
    let namespace_state = NamespaceState {
        namespace: namespace.clone(),
        watcher_shutdown_tx,
        watcher_handle,
    };
    if NAMESPACE_STATE
        .set(std::sync::RwLock::new(namespace_state))
        .is_err()
    {
        error!("Namespace state already initialized");
        return Err(already_initialized_error("Namespace state"));
    }

    info!(
        "Cache initialized with {}MB limit, {} concurrent fetchers, and K8s watch streams",
        DEFAULT_CACHE_SIZE_MB, DEFAULT_CONCURRENT_FETCHERS
    );

    // Direct fetch essential data for immediate UI responsiveness
    let essential_request = DataRequest::ReplicaSets {
        namespace: Some(namespace),
        labels: std::collections::BTreeMap::new(),
    };

    // Fetch ReplicaSet data directly and populate cache immediately
    start_blocking_operation();
    let rs_result = crate::k8s::rs::list_replicas().await;
    end_blocking_operation();
    match rs_result {
        Ok(rs_data) => {
            let fetch_result = FetchResult::ReplicaSets(rs_data);
            if let Err(e) = cache.put(&essential_request, fetch_result).await {
                warn!("Failed to populate cache with ReplicaSet data: {}", e);
            } else {
                info!("🚀 Cache pre-populated with ReplicaSet data for instant UI startup");
            }
        }
        Err(e) => {
            warn!("Failed to fetch initial ReplicaSet data: {}", e);
        }
    }

    Ok(())
}

/// Get the global cache instance
///
/// Returns None if cache hasn't been initialized yet
#[must_use]
pub fn get_cache() -> Option<Arc<K8sDataCache>> {
    CACHE.get().cloned()
}

/// Get the global cache instance, with fallback
///
/// Creates a temporary cache if the global one isn't initialized
#[must_use]
pub fn get_cache_or_default() -> Arc<K8sDataCache> {
    CACHE.get().map_or_else(
        || {
            warn!("Cache not initialized, creating temporary cache");
            Arc::new(K8sDataCache::new(10)) // Smaller temporary cache
        },
        std::clone::Clone::clone,
    )
}

/// Get the current namespace context
///
/// Returns the namespace that was set during cache initialization or switched to
#[must_use]
pub fn get_current_namespace() -> Option<String> {
    Some(NAMESPACE_STATE.get()?.read().ok()?.namespace.clone())
}

/// Get the current namespace with fallback to "default"
#[must_use]
pub fn get_current_namespace_or_default() -> String {
    get_current_namespace().unwrap_or_else(|| "default".to_string())
}

/// Switch to a new namespace at runtime
///
/// This function performs an atomic namespace switch with rollback on failure:
/// 1. Prepare new watch streams for the new namespace (before modifying state)
/// 2. Atomically stop old watches and swap to new state
/// 3. Clear stale cache data
///
/// # Concurrency Safety
///
/// The implementation minimizes the window where state is inconsistent by:
/// - Preparing the new WatchManager before acquiring the write lock
/// - Performing the state swap atomically while holding the lock
/// - Only clearing the cache after the swap is complete
///
/// # Errors
///
/// Returns an error if:
/// - Cache or namespace state is not initialized
/// - Watch manager initialization fails for the new namespace (no state change occurs)
/// - State lock is poisoned (indicates a prior panic)
pub async fn switch_namespace(new_namespace: String) -> Result<()> {
    let cache = get_cache().ok_or_else(|| {
        crate::k8s::cache::errors::cache_not_initialized_error(
            "Cache not initialized for namespace switch",
        )
    })?;

    let state_lock = NAMESPACE_STATE.get().ok_or_else(|| {
        crate::k8s::cache::errors::cache_not_initialized_error("Namespace state not initialized")
    })?;

    // Get old namespace for logging (read-only, brief lock)
    let old_namespace = {
        let state = state_lock.read().map_err(|e| {
            error!("Namespace state lock poisoned during read: {}", e);
            crate::k8s::cache::errors::lock_poisoned_error(
                "Namespace state lock poisoned during read",
            )
        })?;
        state.namespace.clone()
    };

    // Skip if already on this namespace
    if old_namespace == new_namespace {
        info!("Already on namespace '{}', skipping switch", new_namespace);
        return Ok(());
    }

    info!(
        "Switching namespace from '{}' to '{}'",
        old_namespace, new_namespace
    );

    // 1. PREPARE: Create new watch manager BEFORE modifying any state
    // This ensures if creation fails, we haven't touched the existing state
    let watch_manager = match WatchManager::new(cache.clone(), new_namespace.clone()).await {
        Ok(wm) => wm,
        Err(e) => {
            error!(
                "Failed to create watch manager for namespace '{}': {}",
                new_namespace, e
            );
            // No state was modified, safe to return error
            return Err(e);
        }
    };
    let (new_shutdown_tx, new_handle) = watch_manager.start();

    // 2. SWAP: Atomically stop old watches and install new state
    // Hold the write lock for the entire swap operation
    {
        let mut state = state_lock.write().map_err(|e| {
            error!("Namespace state lock poisoned during write: {}", e);
            // New watch manager will be dropped, cleaning up its resources
            crate::k8s::cache::errors::lock_poisoned_error(
                "Namespace state lock poisoned during switch",
            )
        })?;

        // Stop existing watches
        let _ = state.watcher_shutdown_tx.try_send(());
        state.watcher_handle.shutdown_in_place();

        // Install new state atomically
        state.namespace = new_namespace.clone();
        state.watcher_shutdown_tx = new_shutdown_tx;
        state.watcher_handle = new_handle;
    }
    // Lock released here - state is now consistent with new namespace

    // 3. CLEANUP: Clear stale cache data (safe to do outside lock)
    cache.clear().await;

    info!(
        "Successfully switched namespace from '{}' to '{}'",
        old_namespace, new_namespace
    );

    Ok(())
}

/// Get the global background fetcher instance
///
/// Returns None if fetcher hasn't been initialized yet
#[must_use]
pub fn get_background_fetcher() -> Option<Arc<BackgroundFetcher>> {
    BACKGROUND_FETCHER.get().cloned()
}

/// Increment the network activity counter (call before any K8s API operation)
pub fn start_network_operation() {
    NETWORK_ACTIVITY_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let now = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX);
    LAST_NETWORK_TIME.store(now, std::sync::atomic::Ordering::Relaxed);
}

/// Decrement the network activity counter (call after any K8s API operation)
pub fn end_network_operation() {
    NETWORK_ACTIVITY_COUNTER.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
}

/// Increment the blocking activity counter (call before blocking/cache miss operations)
pub fn start_blocking_operation() {
    BLOCKING_ACTIVITY_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let now = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX);
    LAST_BLOCKING_TIME.store(now, std::sync::atomic::Ordering::Relaxed);
    start_network_operation(); // Also count as general network activity
}

/// Decrement the blocking activity counter (call after blocking/cache miss operations)
pub fn end_blocking_operation() {
    BLOCKING_ACTIVITY_COUNTER.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    end_network_operation(); // Also decrement general network activity
}

/// Check if there's any active network IO (what users actually care about).
///
/// Returns true if there are active network operations happening right now
/// OR if network activity happened recently (minimum 500ms display time)
pub fn has_network_activity() -> bool {
    let active_count = NETWORK_ACTIVITY_COUNTER.load(std::sync::atomic::Ordering::Relaxed);
    if active_count > 0 {
        return true;
    }

    // Show for minimum time even after operation completes
    let now = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX);
    let last_activity = LAST_NETWORK_TIME.load(std::sync::atomic::Ordering::Relaxed);

    now.saturating_sub(last_activity) < 500 // Show for 500ms minimum
}

/// Check if there's any blocking network IO (cache misses - should be red!).
///
/// Returns true if there are blocking operations happening (indicates cache problems)
/// OR if blocking activity happened recently (minimum 1000ms display time)
pub fn has_blocking_activity() -> bool {
    let active_count = BLOCKING_ACTIVITY_COUNTER.load(std::sync::atomic::Ordering::Relaxed);
    if active_count > 0 {
        return true;
    }

    // Show for minimum time even after operation completes
    let now = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX);
    let last_activity = LAST_BLOCKING_TIME.load(std::sync::atomic::Ordering::Relaxed);

    now.saturating_sub(last_activity) < 1000 // Show for 1000ms minimum
}

/// Check if there's any background fetch activity for UI indicators
/// Returns true if there are active or queued fetch operations
pub async fn has_background_activity() -> bool {
    if let Some(fetcher) = get_background_fetcher() {
        fetcher.has_activity().await
    } else {
        false
    }
}

/// Get detailed background fetch activity status
/// Returns (`active_fetches`, `queued_fetches`) or (0, 0) if fetcher not available
pub async fn get_background_activity_status() -> (usize, usize) {
    if let Some(fetcher) = get_background_fetcher() {
        fetcher.get_activity_status().await
    } else {
        (0, 0)
    }
}

/// Get the global metrics history store
///
/// Returns None if not yet initialized
#[must_use]
pub fn get_metrics_history() -> Option<&'static Arc<RwLock<MetricsHistoryStore>>> {
    METRICS_HISTORY.get()
}

/// Record pod metrics in history
pub fn record_pod_metrics(pod_name: &str, cpu_millis: Option<f64>, memory_bytes: Option<u64>) {
    if let Some(store) = METRICS_HISTORY.get() {
        if let Ok(mut history) = store.write() {
            history.record_pod_metrics(pod_name, cpu_millis, memory_bytes);
        }
    }
}

/// Record container metrics in history
pub fn record_container_metrics(
    pod_name: &str,
    container_name: &str,
    cpu_millis: Option<f64>,
    memory_bytes: Option<u64>,
) {
    if let Some(store) = METRICS_HISTORY.get() {
        if let Ok(mut history) = store.write() {
            history.record_container_metrics(pod_name, container_name, cpu_millis, memory_bytes);
        }
    }
}

/// Shutdown the cache system (background fetcher and watch manager)
///
/// This should be called on application exit
#[allow(clippy::cognitive_complexity)]
pub async fn shutdown_cache() {
    if let Some(fetcher_shutdown_tx) = FETCHER_SHUTDOWN_TX.get() {
        let _ = fetcher_shutdown_tx.send(()).await;
        info!("Background fetcher shutdown requested");
    }

    // Shutdown watch manager via namespace state
    if let Some(state_lock) = NAMESPACE_STATE.get() {
        if let Ok(state) = state_lock.read() {
            let _ = state.watcher_shutdown_tx.try_send(());
            info!(
                "Watch manager shutdown requested (namespace: {})",
                state.namespace
            );
            info!(
                "Watch manager has {} active tasks that will be cleaned up via shutdown signal",
                state.watcher_handle.task_count()
            );
        }
    }

    if let Some(cleanup_shutdown_tx) = METRICS_CLEANUP_SHUTDOWN_TX.get() {
        let _ = cleanup_shutdown_tx.send(()).await;
        info!("Metrics history cleanup task shutdown requested");
    }
}
