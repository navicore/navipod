use super::cached_data::{CachedData, FetchStatus};
use super::config::DEFAULT_MAX_PREFETCH_REPLICASETS;
use super::fetcher::{DataRequest, FetchResult};
use super::subscription::SubscriptionManager;
use crate::error::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

#[derive(Debug)]
pub struct K8sDataCache {
    cache: Arc<RwLock<HashMap<String, CachedEntry>>>,
    pub subscription_manager: Arc<SubscriptionManager>,
    max_memory_bytes: usize,
    current_memory_bytes: Arc<RwLock<usize>>,
}

#[derive(Debug)]
struct CachedEntry {
    data: FetchResult,
    metadata: CachedData<()>,
    size_bytes: usize,
}

impl K8sDataCache {
    #[must_use]
    pub fn new(max_memory_mb: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            subscription_manager: Arc::new(SubscriptionManager::new()),
            max_memory_bytes: max_memory_mb * 1024 * 1024,
            current_memory_bytes: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn get(&self, request: &DataRequest) -> Option<FetchResult> {
        let key = request.cache_key();
        let cache = self.cache.read().await;

        match cache.get(&key) {
            Some(entry) if entry.metadata.is_fresh() => {
                debug!("🎯 Cache HIT: {}", key);
                Some(entry.data.clone())
            }
            Some(_) => {
                debug!("🔄 Cache STALE: {}", key);
                None
            }
            None => {
                debug!("❌ Cache MISS: {}", key);
                None
            }
        }
    }

    pub async fn get_or_mark_stale(&self, request: &DataRequest) -> Option<FetchResult> {
        let key = request.cache_key();
        let mut cache = self.cache.write().await;

        cache.get_mut(&key).and_then(|entry| {
            if entry.metadata.is_expired() {
                entry.metadata.mark_stale();
                None
            } else {
                Some(entry.data.clone())
            }
        })
    }

    /// Stores data in the cache with the appropriate TTL.
    ///
    /// # Errors
    ///
    /// Returns an error if cache eviction fails when memory limit is exceeded.
    #[allow(clippy::significant_drop_tightening)]
    pub async fn put(&self, request: &DataRequest, data: FetchResult) -> Result<()> {
        let key = request.cache_key();
        let ttl = request.default_ttl();
        let size_bytes = self.estimate_size(&data);

        info!(
            "💾 Cache STORE: {} ({}KB, TTL: {}s)",
            key,
            size_bytes / 1024,
            ttl.as_secs()
        );

        // Check memory limit
        let mut current_size = self.current_memory_bytes.write().await;
        if *current_size + size_bytes > self.max_memory_bytes {
            self.evict_lru().await?;
        }

        let entry = CachedEntry {
            data: data.clone(),
            metadata: CachedData::new((), ttl),
            size_bytes,
        };

        {
            let mut cache = self.cache.write().await;

            // Update memory tracking
            if let Some(old_entry) = cache.get(&key) {
                *current_size = current_size.saturating_sub(old_entry.size_bytes);
            }
            *current_size += size_bytes;

            cache.insert(key.clone(), entry);
        } // Drop cache lock here

        // Notify subscribers
        self.subscription_manager.notify(&key, data).await;

        Ok(())
    }

    pub async fn invalidate(&self, request: &DataRequest) {
        let key = request.cache_key();
        let mut cache = self.cache.write().await;

        if let Some(entry) = cache.get_mut(&key) {
            entry.metadata.mark_stale();
        }
    }

    /// Invalidate all cache entries matching a pattern
    pub async fn invalidate_pattern(&self, pattern: &str) {
        let mut cache = self.cache.write().await;

        // Convert glob-style pattern to regex-like matching
        let pattern_parts: Vec<&str> = pattern.split('*').collect();

        #[allow(clippy::needless_collect)]
        for key in cache.keys().cloned().collect::<Vec<_>>() {
            let matches = if pattern_parts.len() == 1 {
                // No wildcards, exact match
                key == pattern
            } else if pattern_parts.len() == 2 {
                // One wildcard: prefix*suffix pattern
                let prefix = pattern_parts[0];
                let suffix = pattern_parts[1];
                key.starts_with(prefix) && (suffix.is_empty() || key.ends_with(suffix))
            } else {
                // Multiple wildcards - more complex matching
                let mut pos = 0;
                let mut matches_all = true;
                for (i, part) in pattern_parts.iter().enumerate() {
                    if part.is_empty() {
                        continue;
                    }
                    if i == 0 {
                        // First part must match from start
                        if !key[pos..].starts_with(part) {
                            matches_all = false;
                            break;
                        }
                        pos += part.len();
                    } else if i == pattern_parts.len() - 1 {
                        // Last part must match at end
                        if !key[pos..].ends_with(part) {
                            matches_all = false;
                            break;
                        }
                    } else {
                        // Middle parts can match anywhere after current position
                        if let Some(found_pos) = key[pos..].find(part) {
                            pos += found_pos + part.len();
                        } else {
                            matches_all = false;
                            break;
                        }
                    }
                }
                matches_all
            };

            if matches {
                if let Some(entry) = cache.get_mut(&key) {
                    entry.metadata.mark_stale();
                    debug!("🔄 Pattern invalidated: {}", key);
                }
            }
        }
    }

    pub async fn remove(&self, request: &DataRequest) {
        let key = request.cache_key();
        let mut cache = self.cache.write().await;

        if let Some(entry) = cache.remove(&key) {
            let mut current_size = self.current_memory_bytes.write().await;
            *current_size = current_size.saturating_sub(entry.size_bytes);
        }
    }

    pub async fn clear(&self) {
        self.cache.write().await.clear();

        let mut current_size = self.current_memory_bytes.write().await;
        *current_size = 0;
    }

    pub async fn mark_fetching(&self, request: &DataRequest) {
        let key = request.cache_key();
        let mut cache = self.cache.write().await;

        if let Some(entry) = cache.get_mut(&key) {
            entry.metadata.mark_fetching();
        }
    }

    pub async fn mark_error(&self, request: &DataRequest, error: String) {
        let key = request.cache_key();
        let mut cache = self.cache.write().await;

        if let Some(entry) = cache.get_mut(&key) {
            entry.metadata.mark_error(error);
        }
    }

    async fn evict_lru(&self) -> Result<()> {
        let mut cache = self.cache.write().await;

        // Find the least recently used entry
        let oldest_key = cache
            .iter()
            .min_by_key(|(_, entry)| entry.metadata.last_updated)
            .map(|(key, _)| key.clone());

        if let Some(key) = oldest_key {
            if let Some(entry) = cache.remove(&key) {
                let mut current_size = self.current_memory_bytes.write().await;
                *current_size = current_size.saturating_sub(entry.size_bytes);
            }
        }

        Ok(())
    }

    #[allow(clippy::unused_self)]
    #[allow(clippy::missing_const_for_fn)]
    fn estimate_size(&self, data: &FetchResult) -> usize {
        // Rough estimation of memory usage
        match data {
            FetchResult::ReplicaSets(items) => items.len() * 1024,
            FetchResult::Pods(items) => items.len() * 2048,
            FetchResult::Containers(items) => items.len() * 512,
            FetchResult::Events(items) => items.len() * 256,
            FetchResult::Ingresses(items) => items.len() * 1024,
        }
    }

    #[allow(clippy::significant_drop_tightening)]
    pub async fn stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        let current_size = *self.current_memory_bytes.read().await;

        let total_entries = cache.len();
        let fresh_entries = cache.values().filter(|e| e.metadata.is_fresh()).count();
        let stale_entries = cache
            .values()
            .filter(|e| matches!(e.metadata.fetch_status, FetchStatus::Stale))
            .count();
        let error_entries = cache
            .values()
            .filter(|e| matches!(e.metadata.fetch_status, FetchStatus::Error(_)))
            .count();

        CacheStats {
            total_entries,
            fresh_entries,
            stale_entries,
            error_entries,
            memory_used_bytes: current_size,
            memory_limit_bytes: self.max_memory_bytes,
            hit_rate: 0.0, // Will be calculated based on actual hits/misses
        }
    }

    pub async fn get_expired_keys(&self) -> Vec<String> {
        let cache = self.cache.read().await;
        cache
            .iter()
            .filter(|(_, entry)| entry.metadata.is_expired())
            .map(|(key, _)| key.clone())
            .collect()
    }

    pub async fn prefetch_related(&self, request: &DataRequest) -> Vec<DataRequest> {
        use super::fetcher::PodSelector;

        // Determine what related data should be prefetched
        match request {
            DataRequest::ReplicaSets { namespace, .. } => {
                // When fetching ReplicaSets, we should prefetch pods for common selectors
                // This is the core predictive behavior - assume user will drill down
                let namespace = namespace
                    .clone()
                    .unwrap_or_else(|| crate::cache_manager::get_current_namespace_or_default());

                debug!(
                    "🔮 PREFETCH: Generating Pod requests for ReplicaSet namespace: {}",
                    namespace
                );

                // Get the current cached ReplicaSets to extract their selectors
                if let Some(super::fetcher::FetchResult::ReplicaSets(replicasets)) =
                    self.get(request).await
                {
                    let mut prefetch_requests = Vec::new();

                    for rs in replicasets.iter().take(DEFAULT_MAX_PREFETCH_REPLICASETS) {
                        // Limit to avoid overwhelming
                        if let Some(selectors) = &rs.selectors {
                            let pod_request = DataRequest::Pods {
                                namespace: namespace.clone(),
                                selector: PodSelector::ByLabels(selectors.clone()),
                            };
                            prefetch_requests.push(pod_request);
                            debug!(
                                "🔮 PREFETCH: Generated Pod request for RS {} with selectors: {:?}",
                                rs.name, selectors
                            );
                        }
                    }

                    info!(
                        "🔮 PREFETCH: Generated {} Pod requests for ReplicaSet data",
                        prefetch_requests.len()
                    );
                    prefetch_requests
                } else {
                    // If ReplicaSets not in cache yet, don't prefetch Pods blindly
                    // This avoids the performance overhead of fetching all pods
                    debug!("🔮 PREFETCH: No cached ReplicaSets available, skipping Pod prefetch");
                    vec![]
                }
            }
            DataRequest::Pods {
                namespace: _,
                selector: _,
            } => {
                // When fetching Pods, consider prefetching events for the namespace
                vec![DataRequest::Events {
                    resource: super::fetcher::ResourceRef::All,
                    limit: 50,
                }]
            }
            _ => vec![],
        }
    }

    /// Generate prefetch requests using fresh data instead of cached data
    /// This solves the chicken-and-egg problem where we need cached data to generate prefetch requests
    pub fn prefetch_related_with_data(
        &self,
        request: &DataRequest,
        data: &super::fetcher::FetchResult,
    ) -> Vec<DataRequest> {
        use super::fetcher::PodSelector;

        // Determine what related data should be prefetched based on fresh data
        match (request, data) {
            (
                DataRequest::ReplicaSets { namespace, .. },
                super::fetcher::FetchResult::ReplicaSets(replicasets),
            ) => {
                // When we just fetched ReplicaSets, immediately prefetch pods for their selectors
                let namespace = namespace
                    .clone()
                    .unwrap_or_else(|| crate::cache_manager::get_current_namespace_or_default());

                debug!(
                    "🔮 PREFETCH WITH DATA: Generating Pod requests for {} ReplicaSets in namespace: {}",
                    replicasets.len(),
                    namespace
                );

                let mut prefetch_requests = Vec::new();

                for rs in replicasets.iter().take(DEFAULT_MAX_PREFETCH_REPLICASETS) {
                    // Limit to avoid overwhelming
                    if let Some(selectors) = &rs.selectors {
                        let pod_request = DataRequest::Pods {
                            namespace: namespace.clone(),
                            selector: PodSelector::ByLabels(selectors.clone()),
                        };
                        prefetch_requests.push(pod_request);
                        debug!(
                            "🔮 PREFETCH WITH DATA: Generated Pod request for RS {} with selectors: {:?}",
                            rs.name, selectors
                        );
                    }
                }

                info!(
                    "🔮 PREFETCH WITH DATA: Generated {} Pod requests for fresh ReplicaSet data",
                    prefetch_requests.len()
                );
                prefetch_requests
            }
            (
                DataRequest::Pods {
                    namespace: _,
                    selector: _,
                },
                super::fetcher::FetchResult::Pods(_),
            ) => {
                // When fetching Pods, consider prefetching events for the namespace
                vec![DataRequest::Events {
                    resource: super::fetcher::ResourceRef::All,
                    limit: 50,
                }]
            }
            _ => vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub fresh_entries: usize,
    pub stale_entries: usize,
    pub error_entries: usize,
    pub memory_used_bytes: usize,
    pub memory_limit_bytes: usize,
    pub hit_rate: f64,
}

impl CacheStats {
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn memory_usage_percent(&self) -> f64 {
        (self.memory_used_bytes as f64 / self.memory_limit_bytes as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn test_cache_basic_operations() {
        let cache = K8sDataCache::new(10); // 10MB limit

        let request = DataRequest::ReplicaSets {
            namespace: Some("default".to_string()),
            labels: BTreeMap::new(),
        };

        // Test empty cache
        assert!(cache.get(&request).await.is_none());

        // Test put and get
        let data = FetchResult::ReplicaSets(vec![]);
        cache.put(&request, data.clone()).await.unwrap();

        let retrieved = cache.get(&request).await;
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_cache_expiry() {
        let cache = K8sDataCache::new(10);

        let request = DataRequest::Events {
            resource: super::super::fetcher::ResourceRef::Pod("test".to_string()),
            limit: 10,
        };

        let data = FetchResult::Events(vec![]);
        cache.put(&request, data).await.unwrap();

        // Should be fresh initially
        assert!(cache.get(&request).await.is_some());

        // Mark as stale
        cache.invalidate(&request).await;

        // Should return None when stale
        assert!(cache.get(&request).await.is_none());
    }
}
