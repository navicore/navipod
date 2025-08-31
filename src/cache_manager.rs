/**
Global cache manager for `NaviPod`

This module provides a singleton cache that can be accessed throughout the app.
It initializes the cache and background fetcher at startup.
*/
use crate::error::Result;
use crate::k8s::cache::{
    BackgroundFetcher, DataRequest, FetchResult, K8sDataCache, WatchManager, WatchManagerHandle,
};
use std::sync::{Arc, OnceLock};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Global cache instance
static CACHE: OnceLock<Arc<K8sDataCache>> = OnceLock::new();
/// Background fetcher shutdown channel
static FETCHER_SHUTDOWN_TX: OnceLock<mpsc::Sender<()>> = OnceLock::new();
/// Watch manager shutdown channel
static WATCHER_SHUTDOWN_TX: OnceLock<mpsc::Sender<()>> = OnceLock::new();
/// Watch manager task handle
static WATCHER_HANDLE: OnceLock<WatchManagerHandle> = OnceLock::new();
/// Current namespace context
static CURRENT_NAMESPACE: OnceLock<String> = OnceLock::new();

/// Initialize the global cache and background fetcher
///
/// # Errors
///
/// Returns an error if cache is already initialized or if initialization fails
#[allow(clippy::cognitive_complexity)]
pub async fn initialize_cache(namespace: String) -> Result<()> {
    let cache = Arc::new(K8sDataCache::new(100)); // 100MB cache
    let fetcher = BackgroundFetcher::new(cache.clone(), 8); // 8 concurrent fetches

    let (_fetcher_arc, fetcher_shutdown_tx) = fetcher.start();

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

    // Store the namespace and cache globally
    if CURRENT_NAMESPACE.set(namespace.clone()).is_err() {
        error!("Namespace already set");
        return Err(crate::error::Error::Kube(kube::Error::Api(
            kube::error::ErrorResponse {
                status: "AlreadyExists".to_string(),
                message: "Namespace already set".to_string(),
                reason: "AlreadyInitialized".to_string(),
                code: 409,
            },
        )));
    }

    if CACHE.set(cache.clone()).is_err() {
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

    if FETCHER_SHUTDOWN_TX.set(fetcher_shutdown_tx).is_err() {
        error!("Fetcher shutdown channel already initialized");
        return Err(crate::error::Error::Kube(kube::Error::Api(
            kube::error::ErrorResponse {
                status: "AlreadyExists".to_string(),
                message: "Fetcher shutdown channel already initialized".to_string(),
                reason: "AlreadyInitialized".to_string(),
                code: 409,
            },
        )));
    }

    if WATCHER_SHUTDOWN_TX.set(watcher_shutdown_tx).is_err() {
        error!("Watcher shutdown channel already initialized");
        return Err(crate::error::Error::Kube(kube::Error::Api(
            kube::error::ErrorResponse {
                status: "AlreadyExists".to_string(),
                message: "Watcher shutdown channel already initialized".to_string(),
                reason: "AlreadyInitialized".to_string(),
                code: 409,
            },
        )));
    }

    if WATCHER_HANDLE.set(watcher_handle).is_err() {
        error!("Watcher handle already initialized");
        return Err(crate::error::Error::Kube(kube::Error::Api(
            kube::error::ErrorResponse {
                status: "AlreadyExists".to_string(),
                message: "Watcher handle already initialized".to_string(),
                reason: "AlreadyInitialized".to_string(),
                code: 409,
            },
        )));
    }

    info!("Cache initialized with 100MB limit, 8 concurrent fetchers, and K8s watch streams");

    // Direct fetch essential data for immediate UI responsiveness
    let essential_request = DataRequest::ReplicaSets {
        namespace: Some(namespace),
        labels: std::collections::BTreeMap::new(),
    };

    // Fetch ReplicaSet data directly and populate cache immediately
    match crate::k8s::rs::list_replicas().await {
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

    // Cleanup task handles to prevent resource leaks
    if let Some(_watcher_handle) = WATCHER_HANDLE.get() {
        // We can't take ownership from OnceLock, so we'll abort tasks via shutdown signal
        // The handles will be cleaned up when the shutdown signal is received
        info!("Watch manager tasks will be cleaned up via shutdown signal");
    }
}
