use super::cached_data::{CachedData, FetchStatus};
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
        
        info!("💾 Cache STORE: {} ({}KB, TTL: {}s)", key, size_bytes / 1024, ttl.as_secs());

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

    #[allow(clippy::unused_async)]
    pub async fn prefetch_related(&self, request: &DataRequest) -> Vec<DataRequest> {
        // Determine what related data should be prefetched
        match request {
            DataRequest::ReplicaSets { .. } => {
                // When fetching ReplicaSets, prefetch pods
                vec![] // Will be implemented with actual prefetch logic
            }
            DataRequest::Pods { .. } => {
                // When fetching Pods, consider prefetching containers and events
                vec![] // Will be implemented with actual prefetch logic
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
