/**
Global cache manager for `NaviPod`

This module provides a singleton cache that can be accessed throughout the app.
It initializes the cache and background fetcher at startup.
*/
use crate::error::Result;
use crate::k8s::cache::{
    config::{DEFAULT_CACHE_SIZE_MB, DEFAULT_CONCURRENT_FETCHERS},
    errors::already_initialized_error,
    BackgroundFetcher, DataRequest, FetchResult, K8sDataCache, WatchManager, WatchManagerHandle,
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
/// Watch manager shutdown channel
static WATCHER_SHUTDOWN_TX: OnceLock<mpsc::Sender<()>> = OnceLock::new();
/// Watch manager task handle
static WATCHER_HANDLE: OnceLock<WatchManagerHandle> = OnceLock::new();
/// Current namespace context
static CURRENT_NAMESPACE: OnceLock<String> = OnceLock::new();
/// Global metrics history store for trend visualization
static METRICS_HISTORY: OnceLock<Arc<RwLock<MetricsHistoryStore>>> = OnceLock::new();
/// Metrics history cleanup task shutdown channel
static METRICS_CLEANUP_SHUTDOWN_TX: OnceLock<mpsc::Sender<()>> = OnceLock::new();
/// Global counter for active network operations (blocking IO)
static NETWORK_ACTIVITY_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Global counter for blocking network operations (cache misses - should be red!)
static BLOCKING_ACTIVITY_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
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

    if METRICS_CLEANUP_SHUTDOWN_TX.set(cleanup_shutdown_tx).is_err() {
        error!("Metrics cleanup shutdown channel already initialized");
        let _ = fetcher_shutdown_tx.send(()).await;
        let _ = watcher_shutdown_tx.send(()).await;
        return Err(already_initialized_error("Metrics cleanup shutdown channel"));
    }

    // Store all global state atomically to prevent race conditions
    // If any step fails, we need to clean up properly
    if CURRENT_NAMESPACE.set(namespace.clone()).is_err() {
        error!("Namespace already set");
        let _ = fetcher_shutdown_tx.send(()).await;
        let _ = watcher_shutdown_tx.send(()).await;
        if let Some(cleanup_tx) = METRICS_CLEANUP_SHUTDOWN_TX.get() {
            let _ = cleanup_tx.send(()).await;
        }
        return Err(already_initialized_error("Namespace"));
    }

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

    if WATCHER_SHUTDOWN_TX.set(watcher_shutdown_tx).is_err() {
        error!("Watcher shutdown channel already initialized");
        return Err(already_initialized_error("Watcher shutdown channel"));
    }

    if WATCHER_HANDLE.set(watcher_handle).is_err() {
        error!("Watcher handle already initialized");
        return Err(already_initialized_error("Watcher handle"));
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
/// Returns the namespace that was set during cache initialization
#[must_use]
pub fn get_current_namespace() -> Option<String> {
    CURRENT_NAMESPACE.get().cloned()
}

/// Get the current namespace with fallback to "default"
#[must_use]
pub fn get_current_namespace_or_default() -> String {
    get_current_namespace().unwrap_or_else(|| "default".to_string())
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
            .as_millis()
    ).unwrap_or(u64::MAX);
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
            .as_millis()
    ).unwrap_or(u64::MAX);
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
            .as_millis()
    ).unwrap_or(u64::MAX);
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
            .as_millis()
    ).unwrap_or(u64::MAX);
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

    if let Some(watcher_shutdown_tx) = WATCHER_SHUTDOWN_TX.get() {
        let _ = watcher_shutdown_tx.send(()).await;
        info!("Watch manager shutdown requested");
    }

    if let Some(cleanup_shutdown_tx) = METRICS_CLEANUP_SHUTDOWN_TX.get() {
        let _ = cleanup_shutdown_tx.send(()).await;
        info!("Metrics history cleanup task shutdown requested");
    }

    // Note: Task handles are owned by WatchManagerHandle and cannot be directly
    // accessed from OnceLock. They will be cleaned up when shutdown signals are received.
    if let Some(watcher_handle) = WATCHER_HANDLE.get() {
        info!("Watch manager has {} active tasks that will be cleaned up via shutdown signal",
              watcher_handle.task_count());
    }
}
