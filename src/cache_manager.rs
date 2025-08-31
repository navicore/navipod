/**
Global cache manager for `NaviPod`

This module provides a singleton cache that can be accessed throughout the app.
It initializes the cache and background fetcher at startup.
*/
use crate::error::Result;
use crate::k8s::cache::{BackgroundFetcher, K8sDataCache, WatchManager};
use std::sync::{Arc, OnceLock};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Global cache instance
static CACHE: OnceLock<Arc<K8sDataCache>> = OnceLock::new();
/// Background fetcher shutdown channel
static FETCHER_SHUTDOWN_TX: OnceLock<mpsc::Sender<()>> = OnceLock::new();
/// Watch manager shutdown channel
static WATCHER_SHUTDOWN_TX: OnceLock<mpsc::Sender<()>> = OnceLock::new();

/// Initialize the global cache and background fetcher
///
/// # Errors
///
/// Returns an error if cache is already initialized or if initialization fails
#[allow(clippy::cognitive_complexity)]
pub async fn initialize_cache() -> Result<()> {
    let cache = Arc::new(K8sDataCache::new(100)); // 100MB cache
    let fetcher = BackgroundFetcher::new(cache.clone(), 8); // 8 concurrent fetches

    let (_fetcher_arc, fetcher_shutdown_tx) = fetcher.start();
    
    // Initialize watch manager for real-time invalidation
    let watch_manager = WatchManager::new(cache.clone()).await?;
    let watcher_shutdown_tx = watch_manager.start();

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

    info!("Cache initialized with 100MB limit, 8 concurrent fetchers, and K8s watch streams");

    // Note: Prefetching is now handled by the background fetcher automatically
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

/// Shutdown the cache system (background fetcher and watch manager)
///
/// This should be called on application exit
pub async fn shutdown_cache() {
    if let Some(fetcher_shutdown_tx) = FETCHER_SHUTDOWN_TX.get() {
        let _ = fetcher_shutdown_tx.send(()).await;
        info!("Background fetcher shutdown requested");
    }
    
    if let Some(watcher_shutdown_tx) = WATCHER_SHUTDOWN_TX.get() {
        let _ = watcher_shutdown_tx.send(()).await;
        info!("Watch manager shutdown requested");
    }
}

