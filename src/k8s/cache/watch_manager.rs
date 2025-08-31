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
    Update { request: DataRequest, data: FetchResult },
}

/// Manages K8s watch streams for real-time cache invalidation
pub struct WatchManager {
    cache: Arc<K8sDataCache>,
    client: Client,
    namespace: String,
    shutdown_tx: Option<mpsc::Sender<()>>,
    invalidation_tx: mpsc::Sender<InvalidationEvent>,
    invalidation_rx: Option<mpsc::Receiver<InvalidationEvent>>,
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

        Ok(Self {
            cache,
            client,
            namespace,
            shutdown_tx: None,
            invalidation_tx,
            invalidation_rx: Some(invalidation_rx),
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
    pub fn start(mut self) -> mpsc::Sender<()> {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx.clone());

        // Take the invalidation receiver before moving self
        let invalidation_rx = self.invalidation_rx.take()
            .expect("invalidation_rx should be available during start()");
        let invalidation_tx = self.invalidation_tx.clone();
        let cache = self.cache.clone();
        let client = self.client.clone();

        // Start the invalidation processor
        tokio::spawn(async move {
            Self::run_invalidation_processor(cache, invalidation_rx, shutdown_rx).await;
        });

        // Start individual watch streams (namespace-scoped)
        let namespace = self.namespace.clone();
        Self::start_pod_watcher(client.clone(), invalidation_tx.clone(), namespace.clone());
        Self::start_replicaset_watcher(client.clone(), invalidation_tx.clone(), namespace.clone());
        Self::start_event_watcher(client, invalidation_tx, namespace);

        info!("🔍 Watch streams started for Pods, ReplicaSets, and Events");
        
        shutdown_tx
    }

    /// Process invalidation events from watch streams
    async fn run_invalidation_processor(
        cache: Arc<K8sDataCache>,
        mut invalidation_rx: mpsc::Receiver<InvalidationEvent>,
        mut shutdown_rx: mpsc::Receiver<()>
    ) {
        info!("📡 Invalidation processor started");

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("📡 Invalidation processor shutting down");
                    break;
                }
                Some(event) = invalidation_rx.recv() => {
                    Self::handle_invalidation_event(&cache, event).await;
                }
            }
        }
    }

    /// Handle a single invalidation event
    async fn handle_invalidation_event(cache: &Arc<K8sDataCache>, event: InvalidationEvent) {
        match event {
            InvalidationEvent::Pattern(pattern) => {
                debug!("🔄 INVALIDATE PATTERN: {}", pattern);
                // TODO: Implement pattern-based invalidation
                // This would invalidate all cache keys matching the pattern
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
    fn start_pod_watcher(client: Client, invalidation_tx: mpsc::Sender<InvalidationEvent>, namespace: String) {

        tokio::spawn(async move {
            info!("🔍 Starting Pod watcher");
            
            loop {
                match Self::watch_pods(client.clone(), invalidation_tx.clone(), namespace.clone()).await {
                    Ok(()) => {
                        info!("🔍 Pod watcher stream ended, restarting...");
                    }
                    Err(e) => {
                        error!("❌ Pod watcher failed: {}, restarting in 5s", e);
                        sleep(Duration::from_secs(5)).await;
                    }
                }
                
                sleep(Duration::from_secs(1)).await; // Brief delay before restart
            }
        });
    }

    /// Start watching `ReplicaSet` resources  
    fn start_replicaset_watcher(client: Client, invalidation_tx: mpsc::Sender<InvalidationEvent>, namespace: String) {

        tokio::spawn(async move {
            info!("🔍 Starting ReplicaSet watcher");
            
            loop {
                match Self::watch_replicasets(client.clone(), invalidation_tx.clone(), namespace.clone()).await {
                    Ok(()) => {
                        info!("🔍 ReplicaSet watcher stream ended, restarting...");
                    }
                    Err(e) => {
                        error!("❌ ReplicaSet watcher failed: {}, restarting in 5s", e);
                        sleep(Duration::from_secs(5)).await;
                    }
                }
                
                sleep(Duration::from_secs(1)).await;
            }
        });
    }

    /// Start watching Event resources
    fn start_event_watcher(client: Client, invalidation_tx: mpsc::Sender<InvalidationEvent>, namespace: String) {

        tokio::spawn(async move {
            info!("🔍 Starting Event watcher");
            
            loop {
                match Self::watch_events(client.clone(), invalidation_tx.clone(), namespace.clone()).await {
                    Ok(()) => {
                        info!("🔍 Event watcher stream ended, restarting...");
                    }
                    Err(e) => {
                        error!("❌ Event watcher failed: {}, restarting in 5s", e);
                        sleep(Duration::from_secs(5)).await;
                    }
                }
                
                sleep(Duration::from_secs(1)).await;
            }
        });
    }

    /// Watch Pod resources and send invalidation events
    async fn watch_pods(
        client: Client, 
        invalidation_tx: mpsc::Sender<InvalidationEvent>,
        namespace: String
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
                        let _ = invalidation_tx.send(InvalidationEvent::Pattern(pattern)).await;
                        info!("➕ Pod added: {}/{}", ns, pod.name_any());
                    }
                }
                WatchEvent::Modified(pod) => {
                    if let Some(ns) = pod.namespace() {
                        let pattern = format!("pods:{ns}:*");
                        let _ = invalidation_tx.send(InvalidationEvent::Pattern(pattern)).await;
                        debug!("📝 Pod modified: {}/{}", ns, pod.name_any());
                    }
                }
                WatchEvent::Deleted(pod) => {
                    if let Some(ns) = pod.namespace() {
                        let pattern = format!("pods:{ns}:*");
                        let _ = invalidation_tx.send(InvalidationEvent::Pattern(pattern)).await;
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
        namespace: String
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
                    let _ = invalidation_tx.send(InvalidationEvent::Pattern(pattern)).await;
                    info!("➕ ReplicaSet added: {}/{}", ns, rs.name_any());
                }
                WatchEvent::Modified(rs) => {
                    let ns = rs.namespace().unwrap_or_default();
                    let pattern = format!("rs:{ns}:*");
                    let _ = invalidation_tx.send(InvalidationEvent::Pattern(pattern)).await;
                    debug!("📝 ReplicaSet modified: {}/{}", ns, rs.name_any());
                }
                WatchEvent::Deleted(rs) => {
                    let ns = rs.namespace().unwrap_or_default();
                    let pattern = format!("rs:{ns}:*");
                    let _ = invalidation_tx.send(InvalidationEvent::Pattern(pattern)).await;
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
        namespace: String
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
                    let _ = invalidation_tx.send(InvalidationEvent::Pattern(pattern)).await;
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
    pub const fn stats(&self) -> WatchStats {
        WatchStats {
            active_watchers: 3, // Pod, ReplicaSet, Event
            total_invalidations: 0, // TODO: Track this
            connection_status: WatchConnectionStatus::Connected,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_key_parsing() {
        let _ = rustls::crypto::CryptoProvider::install_default(
            rustls::crypto::ring::default_provider()
        );
        
        let cache = Arc::new(K8sDataCache::new(10));
        let _manager = WatchManager::new(cache, "default".to_string()).await.unwrap();

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