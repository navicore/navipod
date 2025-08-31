/**
 * K8s Watch Stream Manager
 *
 * Manages multiple K8s watch streams for real-time cache invalidation.
 * Provides surgical cache updates based on K8s resource events.
 */
use super::data_cache::K8sDataCache;
use super::fetcher::{DataRequest, FetchResult};
use crate::error::Result;
// use crate::tui::data::{Rs, RsPod};
use k8s_openapi::api::apps::v1::ReplicaSet;
use k8s_openapi::api::core::v1::{Event, Pod};
use kube::api::{Api, WatchEvent, WatchParams};
use kube::{Client, ResourceExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Represents different types of K8s resources we can watch
#[derive(Debug, Clone)]
pub enum WatchedResource {
    Pods,
    ReplicaSets,
    Events,
}

/// Represents a cache invalidation event from K8s watch streams
#[derive(Debug, Clone)]
pub enum InvalidationEvent {
    /// Invalidate all cache entries matching a pattern
    Pattern(String),
    /// Invalidate specific cache key
    Key(String),
    /// Update cache with fresh data
    Update {
        request: DataRequest,
        data: FetchResult,
    },
}

/// Manages K8s watch streams for real-time cache invalidation
pub struct WatchManager {
    cache: Arc<K8sDataCache>,
    client: Client,
    namespace: String,
    shutdown_tx: Option<mpsc::Sender<()>>,
    invalidation_tx: mpsc::Sender<InvalidationEvent>,
    invalidation_rx: Option<mpsc::Receiver<InvalidationEvent>>,
    stats: Arc<std::sync::RwLock<WatchStats>>,
    #[allow(dead_code)]
    task_handles: Vec<tokio::task::JoinHandle<()>>,
}

impl WatchManager {
    /// Create a new `WatchManager`
    ///
    /// # Errors
    ///
    /// Returns an error if K8s client creation fails
    pub async fn new(cache: Arc<K8sDataCache>, namespace: String) -> Result<Self> {
        let client = Client::try_default().await?;
        let (invalidation_tx, invalidation_rx) = mpsc::channel(100);
        let stats = Arc::new(std::sync::RwLock::new(WatchStats {
            active_watchers: 3,
            total_invalidations: 0,
            connection_status: WatchConnectionStatus::Connected,
        }));

        Ok(Self {
            cache,
            client,
            namespace,
            shutdown_tx: None,
            invalidation_tx,
            invalidation_rx: Some(invalidation_rx),
            stats,
            task_handles: Vec::new(),
        })
    }

    /// Start all watch streams
    ///
    /// # Panics
    ///
    /// Panics if the invalidation receiver has already been taken
    ///
    /// # Errors
    ///
    /// This method does not return errors but the watch streams may fail internally
    ///
    /// Returns a shutdown sender that can be used to stop all watches
    #[allow(clippy::expect_used)]
    pub fn start(mut self) -> (mpsc::Sender<()>, WatchManagerHandle) {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx.clone());

        // Take the invalidation receiver before moving self
        let invalidation_rx = self
            .invalidation_rx
            .take()
            .expect("invalidation_rx should be available during start()");
        let invalidation_tx = self.invalidation_tx.clone();
        let cache = self.cache.clone();
        let client = self.client.clone();

        // Start the invalidation processor
        let stats_clone = self.stats.clone();
        let processor_handle = tokio::spawn(async move {
            Self::run_invalidation_processor(cache, invalidation_rx, shutdown_rx, stats_clone)
                .await;
        });

        // Start individual watch streams (namespace-scoped)
        let namespace = self.namespace.clone();
        let pod_handle =
            Self::start_pod_watcher(client.clone(), invalidation_tx.clone(), namespace.clone());
        let rs_handle = Self::start_replicaset_watcher(
            client.clone(),
            invalidation_tx.clone(),
            namespace.clone(),
        );
        let event_handle = Self::start_event_watcher(client, invalidation_tx, namespace);

        info!("🔍 Watch streams started for Pods, ReplicaSets, and Events");

        let handle = WatchManagerHandle {
            task_handles: vec![processor_handle, pod_handle, rs_handle, event_handle],
        };

        (shutdown_tx, handle)
    }

    /// Process invalidation events from watch streams
    async fn run_invalidation_processor(
        cache: Arc<K8sDataCache>,
        mut invalidation_rx: mpsc::Receiver<InvalidationEvent>,
        mut shutdown_rx: mpsc::Receiver<()>,
        stats: Arc<std::sync::RwLock<WatchStats>>,
    ) {
        info!("📡 Invalidation processor started");

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("📡 Invalidation processor shutting down");
                    break;
                }
                Some(event) = invalidation_rx.recv() => {
                    Self::handle_invalidation_event(&cache, event, &stats).await;
                }
            }
        }
    }

    /// Handle a single invalidation event
    async fn handle_invalidation_event(
        cache: &Arc<K8sDataCache>,
        event: InvalidationEvent,
        stats: &Arc<std::sync::RwLock<WatchStats>>,
    ) {
        // Increment invalidation counter
        if let Ok(mut stats) = stats.write() {
            stats.total_invalidations += 1;
        }

        match event {
            InvalidationEvent::Pattern(pattern) => {
                debug!("🔄 INVALIDATE PATTERN: {}", pattern);
                cache.invalidate_pattern(&pattern).await;
            }
            InvalidationEvent::Key(key) => {
                debug!("🔄 INVALIDATE KEY: {}", key);
                // Parse key back to DataRequest for invalidation
                if let Some(request) = Self::parse_cache_key(&key) {
                    cache.invalidate(&request).await;
                }
            }
            InvalidationEvent::Update { request, data } => {
                debug!("⚡ WATCH UPDATE: {}", request.cache_key());
                // Direct update from watch stream with fresh data
                if let Err(e) = cache.put(&request, data).await {
                    warn!("Failed to update cache from watch: {}", e);
                }
            }
        }
    }

    /// Start watching Pod resources
    fn start_pod_watcher(
        client: Client,
        invalidation_tx: mpsc::Sender<InvalidationEvent>,
        namespace: String,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            const MAX_RESTARTS: u32 = 50;
            const MAX_BACKOFF: u64 = 60;
            info!("🔍 Starting Pod watcher");

            let mut backoff_seconds = 1;
            let mut restart_count = 0;

            loop {
                if restart_count >= MAX_RESTARTS {
                    error!(
                        "❌ Pod watcher exceeded maximum restart attempts ({}), stopping",
                        MAX_RESTARTS
                    );
                    break;
                }

                match Self::watch_pods(client.clone(), invalidation_tx.clone(), namespace.clone())
                    .await
                {
                    Ok(()) => {
                        info!("🔍 Pod watcher stream ended normally, restarting...");
                        backoff_seconds = 1; // Reset backoff on successful run
                        restart_count = 0; // Reset restart count on successful run
                    }
                    Err(e) => {
                        restart_count += 1;
                        error!(
                            "❌ Pod watcher failed (attempt {}/{}): {}, restarting in {}s",
                            restart_count, MAX_RESTARTS, e, backoff_seconds
                        );
                        sleep(Duration::from_secs(backoff_seconds)).await;
                        backoff_seconds = (backoff_seconds * 2).min(MAX_BACKOFF);
                        // Exponential backoff
                    }
                }

                sleep(Duration::from_secs(1)).await; // Brief delay before restart
            }
        })
    }

    /// Start watching `ReplicaSet` resources
    fn start_replicaset_watcher(
        client: Client,
        invalidation_tx: mpsc::Sender<InvalidationEvent>,
        namespace: String,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            const MAX_RESTARTS: u32 = 50;
            const MAX_BACKOFF: u64 = 60;
            info!("🔍 Starting ReplicaSet watcher");

            let mut backoff_seconds = 1;
            let mut restart_count = 0;

            loop {
                if restart_count >= MAX_RESTARTS {
                    error!(
                        "❌ ReplicaSet watcher exceeded maximum restart attempts ({}), stopping",
                        MAX_RESTARTS
                    );
                    break;
                }

                match Self::watch_replicasets(
                    client.clone(),
                    invalidation_tx.clone(),
                    namespace.clone(),
                )
                .await
                {
                    Ok(()) => {
                        info!("🔍 ReplicaSet watcher stream ended normally, restarting...");
                        backoff_seconds = 1; // Reset backoff on successful run
                        restart_count = 0; // Reset restart count on successful run
                    }
                    Err(e) => {
                        restart_count += 1;
                        error!(
                            "❌ ReplicaSet watcher failed (attempt {}/{}): {}, restarting in {}s",
                            restart_count, MAX_RESTARTS, e, backoff_seconds
                        );
                        sleep(Duration::from_secs(backoff_seconds)).await;
                        backoff_seconds = (backoff_seconds * 2).min(MAX_BACKOFF);
                        // Exponential backoff
                    }
                }

                sleep(Duration::from_secs(1)).await;
            }
        })
    }

    /// Start watching Event resources
    fn start_event_watcher(
        client: Client,
        invalidation_tx: mpsc::Sender<InvalidationEvent>,
        namespace: String,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            const MAX_RESTARTS: u32 = 50;
            const MAX_BACKOFF: u64 = 60;
            info!("🔍 Starting Event watcher");

            let mut backoff_seconds = 1;
            let mut restart_count = 0;

            loop {
                if restart_count >= MAX_RESTARTS {
                    error!(
                        "❌ Event watcher exceeded maximum restart attempts ({}), stopping",
                        MAX_RESTARTS
                    );
                    break;
                }

                match Self::watch_events(client.clone(), invalidation_tx.clone(), namespace.clone())
                    .await
                {
                    Ok(()) => {
                        info!("🔍 Event watcher stream ended normally, restarting...");
                        backoff_seconds = 1; // Reset backoff on successful run
                        restart_count = 0; // Reset restart count on successful run
                    }
                    Err(e) => {
                        restart_count += 1;
                        error!(
                            "❌ Event watcher failed (attempt {}/{}): {}, restarting in {}s",
                            restart_count, MAX_RESTARTS, e, backoff_seconds
                        );
                        sleep(Duration::from_secs(backoff_seconds)).await;
                        backoff_seconds = (backoff_seconds * 2).min(MAX_BACKOFF);
                        // Exponential backoff
                    }
                }

                sleep(Duration::from_secs(1)).await;
            }
        })
    }

    /// Watch Pod resources and send invalidation events
    async fn watch_pods(
        client: Client,
        invalidation_tx: mpsc::Sender<InvalidationEvent>,
        namespace: String,
    ) -> Result<()> {
        use futures::{pin_mut, TryStreamExt};

        let pods: Api<Pod> = Api::namespaced(client, &namespace);
        let wp = WatchParams::default().timeout(294); // 5 minute timeout

        let stream = pods.watch(&wp, "0").await?;
        pin_mut!(stream);

        while let Some(event) = stream.try_next().await? {
            match event {
                WatchEvent::Added(pod) => {
                    if let Some(ns) = pod.namespace() {
                        let pattern = format!("pods:{ns}:*");
                        let _ = invalidation_tx
                            .send(InvalidationEvent::Pattern(pattern))
                            .await;
                        info!("➕ Pod added: {}/{}", ns, pod.name_any());
                    }
                }
                WatchEvent::Modified(pod) => {
                    if let Some(ns) = pod.namespace() {
                        let pattern = format!("pods:{ns}:*");
                        let _ = invalidation_tx
                            .send(InvalidationEvent::Pattern(pattern))
                            .await;
                        debug!("📝 Pod modified: {}/{}", ns, pod.name_any());
                    }
                }
                WatchEvent::Deleted(pod) => {
                    if let Some(ns) = pod.namespace() {
                        let pattern = format!("pods:{ns}:*");
                        let _ = invalidation_tx
                            .send(InvalidationEvent::Pattern(pattern))
                            .await;
                        info!("🗑️  Pod deleted: {}/{}", ns, pod.name_any());
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Watch `ReplicaSet` resources and send invalidation events
    async fn watch_replicasets(
        client: Client,
        invalidation_tx: mpsc::Sender<InvalidationEvent>,
        namespace: String,
    ) -> Result<()> {
        use futures::{pin_mut, TryStreamExt};

        let replicasets: Api<ReplicaSet> = Api::namespaced(client, &namespace);
        let wp = WatchParams::default().timeout(294);

        let stream = replicasets.watch(&wp, "0").await?;
        pin_mut!(stream);

        while let Some(event) = stream.try_next().await? {
            match event {
                WatchEvent::Added(rs) => {
                    let ns = rs.namespace().unwrap_or_default();
                    let pattern = format!("rs:{ns}:*");
                    let _ = invalidation_tx
                        .send(InvalidationEvent::Pattern(pattern))
                        .await;
                    info!("➕ ReplicaSet added: {}/{}", ns, rs.name_any());
                }
                WatchEvent::Modified(rs) => {
                    let ns = rs.namespace().unwrap_or_default();
                    let pattern = format!("rs:{ns}:*");
                    let _ = invalidation_tx
                        .send(InvalidationEvent::Pattern(pattern))
                        .await;
                    debug!("📝 ReplicaSet modified: {}/{}", ns, rs.name_any());
                }
                WatchEvent::Deleted(rs) => {
                    let ns = rs.namespace().unwrap_or_default();
                    let pattern = format!("rs:{ns}:*");
                    let _ = invalidation_tx
                        .send(InvalidationEvent::Pattern(pattern))
                        .await;
                    info!("🗑️  ReplicaSet deleted: {}/{}", ns, rs.name_any());
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Watch Event resources and send invalidation events
    async fn watch_events(
        client: Client,
        invalidation_tx: mpsc::Sender<InvalidationEvent>,
        namespace: String,
    ) -> Result<()> {
        use futures::{pin_mut, TryStreamExt};

        let events: Api<Event> = Api::namespaced(client, &namespace);
        let wp = WatchParams::default().timeout(294);

        let stream = events.watch(&wp, "0").await?;
        pin_mut!(stream);

        while let Some(event) = stream.try_next().await? {
            match event {
                WatchEvent::Added(_) | WatchEvent::Modified(_) => {
                    // Events change frequently, invalidate event caches
                    let pattern = "events:*".to_string();
                    let _ = invalidation_tx
                        .send(InvalidationEvent::Pattern(pattern))
                        .await;
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Parse a cache key back into a `DataRequest` for invalidation
    fn parse_cache_key(key: &str) -> Option<DataRequest> {
        let parts: Vec<&str> = key.split(':').collect();

        match *(parts.first()?) {
            "rs" => Some(DataRequest::ReplicaSets {
                namespace: if parts.get(1)? == &"all" {
                    None
                } else {
                    Some(parts[1].to_string())
                },
                labels: std::collections::BTreeMap::new(),
            }),
            "pods" => Some(DataRequest::Pods {
                namespace: (*parts.get(1)?).to_string(),
                selector: crate::k8s::cache::PodSelector::All,
            }),
            _ => None,
        }
    }

    /// Get statistics about active watch streams
    #[must_use]
    pub fn stats(&self) -> WatchStats {
        self.stats.read().map_or(
            WatchStats {
                active_watchers: 3,
                total_invalidations: 0,
                connection_status: WatchConnectionStatus::Disconnected,
            },
            |stats| stats.clone(),
        )
    }
}

#[derive(Debug, Clone)]
pub struct WatchStats {
    pub active_watchers: usize,
    pub total_invalidations: u64,
    pub connection_status: WatchConnectionStatus,
}

#[derive(Debug, Clone)]
pub enum WatchConnectionStatus {
    Connected,
    Disconnected,
    Reconnecting,
}

/// Handle to manage watch manager tasks
pub struct WatchManagerHandle {
    task_handles: Vec<tokio::task::JoinHandle<()>>,
}

impl WatchManagerHandle {
    /// Shutdown all watch manager tasks
    pub fn shutdown(self) {
        for handle in self.task_handles {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_key_parsing() {
        let _ = rustls::crypto::CryptoProvider::install_default(
            rustls::crypto::ring::default_provider(),
        );

        let cache = Arc::new(K8sDataCache::new(10));
        let _manager = WatchManager::new(cache, "default".to_string())
            .await
            .unwrap();

        // Test ReplicaSet key parsing
        let rs_key = "rs:all:{}";
        let parsed = WatchManager::parse_cache_key(rs_key);
        assert!(parsed.is_some());
        if let Some(DataRequest::ReplicaSets { namespace, .. }) = parsed {
            assert_eq!(namespace, None);
        }

        // Test Pod key parsing
        let pod_key = "pods:default:All";
        let parsed = WatchManager::parse_cache_key(pod_key);
        assert!(parsed.is_some());
        if let Some(DataRequest::Pods { namespace, .. }) = parsed {
            assert_eq!(namespace, "default");
        }
    }
}
