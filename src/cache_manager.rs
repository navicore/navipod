/**
Global cache manager for `NaviPod`

This module provides a singleton cache that can be accessed throughout the app.
It initializes the cache and background fetcher at startup.
*/
use crate::error::Result;
use crate::k8s::cache::{BackgroundFetcher, K8sDataCache};
use std::sync::{Arc, OnceLock};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Global cache instance
static CACHE: OnceLock<Arc<K8sDataCache>> = OnceLock::new();
/// Background fetcher shutdown channel
static SHUTDOWN_TX: OnceLock<mpsc::Sender<()>> = OnceLock::new();

/// Initialize the global cache and background fetcher
///
/// # Errors
///
/// Returns an error if cache is already initialized or if initialization fails
pub async fn initialize_cache() -> Result<()> {
    let cache = Arc::new(K8sDataCache::new(100)); // 100MB cache
    let fetcher = BackgroundFetcher::new(cache.clone(), 8); // 8 concurrent fetches

    let (fetcher_arc, shutdown_tx) = fetcher.start();

    // Store the cache globally
    if CACHE.set(cache).is_err() {
        error!("Cache already initialized");
        return Err(crate::error::Error::Kube(kube::Error::Api(
            kube::error::ErrorResponse {
                status: "AlreadyExists".to_string(),
                message: "Cache already initialized".to_string(),
                reason: "AlreadyInitialized".to_string(),
                code: 409,
            },
        )));
    }

    if SHUTDOWN_TX.set(shutdown_tx).is_err() {
        error!("Shutdown channel already initialized");
        return Err(crate::error::Error::Kube(kube::Error::Api(
            kube::error::ErrorResponse {
                status: "AlreadyExists".to_string(),
                message: "Shutdown channel already initialized".to_string(),
                reason: "AlreadyInitialized".to_string(),
                code: 409,
            },
        )));
    }

    info!("Cache initialized with 100MB limit and 8 concurrent fetchers");

    // Start prefetching common data
    initial_prefetch(fetcher_arc).await;
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

/// Shutdown the background fetcher
///
/// This should be called on application exit
pub async fn shutdown_cache() {
    if let Some(shutdown_tx) = SHUTDOWN_TX.get() {
        let _ = shutdown_tx.send(()).await;
        info!("Cache shutdown requested");
    }
}

/// Start prefetching commonly accessed data
async fn initial_prefetch(fetcher: Arc<BackgroundFetcher>) {
    use crate::k8s::cache::{DataRequest, FetchPriority};
    use std::collections::BTreeMap;

    // Prefetch all ReplicaSets (most common starting view)
    let rs_request = DataRequest::ReplicaSets {
        namespace: None,
        labels: BTreeMap::new(),
    };
    fetcher
        .schedule_fetch(rs_request, FetchPriority::High)
        .await;

    // Prefetch default namespace pods
    let pod_request = DataRequest::Pods {
        namespace: "default".to_string(),
        selector: crate::k8s::cache::PodSelector::All,
    };
    fetcher
        .schedule_fetch(pod_request, FetchPriority::Medium)
        .await;

    info!("Initial prefetch scheduled");
}
