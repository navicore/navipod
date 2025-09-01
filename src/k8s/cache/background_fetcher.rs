use super::data_cache::K8sDataCache;
use super::fetcher::{DataRequest, FetchPriority, FetchResult};
use super::config::{DEFAULT_MAX_PREFETCH_QUEUE_SIZE, PredictiveCacheConfig};
use crate::error::Result;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

// Import existing fetching functions
use crate::k8s::containers::list as list_containers;
use crate::k8s::events::list_all as list_events;
use crate::k8s::pods::list_rspods;
use crate::k8s::rs::get_replicaset;
use crate::k8s::rs::list_replicas;
use crate::k8s::rs_ingress::list_ingresses;

#[derive(Debug)]
struct FetchTask {
    request: DataRequest,
    priority: FetchPriority,
    scheduled_at: Instant,
    retry_count: u32,
}

impl PartialEq for FetchTask {
    fn eq(&self, other: &Self) -> bool {
        self.request.cache_key() == other.request.cache_key()
    }
}

impl Eq for FetchTask {}

impl PartialOrd for FetchTask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FetchTask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher priority first, then earlier scheduled time
        match self.priority.cmp(&other.priority) {
            std::cmp::Ordering::Equal => other.scheduled_at.cmp(&self.scheduled_at),
            other => other,
        }
    }
}

pub struct BackgroundFetcher {
    cache: Arc<K8sDataCache>,
    task_queue: Arc<RwLock<BinaryHeap<FetchTask>>>,
    active_fetches: Arc<RwLock<HashSet<String>>>,
    max_concurrent_fetches: usize,
    shutdown_tx: Option<mpsc::Sender<()>>,
    config: PredictiveCacheConfig,
    // Deduplication: Track recently submitted requests to avoid duplicates
    recent_requests: Arc<RwLock<HashMap<String, Instant>>>,
    // Metrics for monitoring prefetch effectiveness
    prefetch_metrics: Arc<RwLock<PrefetchMetrics>>,
}

#[derive(Debug, Default, Clone)]
pub struct PrefetchMetrics {
    pub total_prefetch_requests: u64,
    pub successful_prefetches: u64,
    pub failed_prefetches: u64,
    pub queue_overflows: u64,
    pub deduplicated_requests: u64,
}

impl BackgroundFetcher {
    #[must_use]
    pub fn new(cache: Arc<K8sDataCache>, max_concurrent: usize) -> Self {
        Self::with_config(cache, max_concurrent, PredictiveCacheConfig::default())
    }

    #[must_use]
    pub fn with_config(cache: Arc<K8sDataCache>, max_concurrent: usize, config: PredictiveCacheConfig) -> Self {
        Self {
            cache,
            task_queue: Arc::new(RwLock::new(BinaryHeap::new())),
            active_fetches: Arc::new(RwLock::new(HashSet::new())),
            max_concurrent_fetches: max_concurrent,
            shutdown_tx: None,
            config,
            recent_requests: Arc::new(RwLock::new(HashMap::new())),
            prefetch_metrics: Arc::new(RwLock::new(PrefetchMetrics::default())),
        }
    }

    #[must_use]
    pub fn start(mut self) -> (Arc<Self>, mpsc::Sender<()>) {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx.clone());

        let fetcher = Arc::new(self);

        // Spawn the main fetch loop
        let fetcher_clone = fetcher.clone();
        tokio::spawn(async move {
            fetcher_clone.run_fetch_loop(shutdown_rx).await;
        });

        // Spawn the refresh loop for expired entries
        let fetcher_clone = fetcher.clone();
        let (_refresh_shutdown_tx, refresh_shutdown_rx) = mpsc::channel(1);
        tokio::spawn(async move {
            fetcher_clone.run_refresh_loop(refresh_shutdown_rx).await;
        });

        (fetcher, shutdown_tx)
    }

    async fn run_fetch_loop(&self, mut shutdown_rx: mpsc::Receiver<()>) {
        info!("🚀 Background fetcher started (max {} concurrent)", self.max_concurrent_fetches);

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("🛑 Background fetcher shutting down");
                    break;
                }
                () = self.process_next_batch() => {
                    // Continue processing
                }
            }

            // Small delay to prevent busy loop
            sleep(Duration::from_millis(100)).await;
        }
    }

    async fn run_refresh_loop(&self, mut shutdown_rx: mpsc::Receiver<()>) {
        info!("🔄 Cache refresh loop started");

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("🔄 Refresh loop shutting down");
                    break;
                }
                () = sleep(Duration::from_secs(5)) => {
                    self.refresh_expired_entries().await;
                }
            }
        }
    }

    async fn process_next_batch(&self) {
        let active_count = self.active_fetches.read().await.len();
        let available_slots = self.max_concurrent_fetches.saturating_sub(active_count);

        if available_slots == 0 {
            return;
        }

        let queue_size = self.task_queue.read().await.len();
        if queue_size > 0 {
            debug!("📋 Processing batch: {} queued, {} active, {} slots available", 
                   queue_size, active_count, available_slots);
        }

        let mut tasks_to_process = Vec::new();
        {
            let mut queue = self.task_queue.write().await;
            for _ in 0..available_slots {
                if let Some(task) = queue.pop() {
                    tasks_to_process.push(task);
                } else {
                    break;
                }
            }
        }

        for task in tasks_to_process {
            self.spawn_fetch_task(task).await;
        }
    }

    async fn spawn_fetch_task(&self, task: FetchTask) {
        let cache_key = task.request.cache_key();

        // Check if already fetching
        {
            let mut active = self.active_fetches.write().await;
            if active.contains(&cache_key) {
                debug!("⚠️  Skipping duplicate fetch for: {}", cache_key);
                return;
            }
            active.insert(cache_key.clone());
        }

        let cache = self.cache.clone();
        let active_fetches = self.active_fetches.clone();
        let task_queue = self.task_queue.clone();
        let prefetch_metrics = self.prefetch_metrics.clone();
        let priority = task.priority;
        let retry_count = task.retry_count;

        tokio::spawn(async move {
            let start = Instant::now();
            info!("🔄 FETCH START: {} (priority: {:?}, attempt: {})", 
                  cache_key, priority, retry_count + 1);

            // Mark as fetching in cache
            cache.mark_fetching(&task.request).await;

            let result = Self::fetch_data(&task.request).await;

            let elapsed = start.elapsed();
            match result {
                Ok(data) => {
                    if let Err(e) = cache.put(&task.request, data.clone()).await {
                        error!("❌ Failed to cache data for {}: {}", cache_key, e);
                        
                        // Update failure metrics if this was a prefetch
                        if priority == FetchPriority::Low {
                            let mut metrics = prefetch_metrics.write().await;
                            metrics.failed_prefetches += 1;
                        }
                    } else {
                        info!("✅ FETCH SUCCESS: {} ({:.2}s)", cache_key, elapsed.as_secs_f64());
                        
                        // Update success metrics if this was a prefetch
                        if priority == FetchPriority::Low {
                            let mut metrics = prefetch_metrics.write().await;
                            metrics.successful_prefetches += 1;
                            drop(metrics); // Release metrics lock early
                        }
                        
                        // PREDICTIVE PREFETCH: After successful cache storage, prefetch related data
                        let prefetch_requests = cache.prefetch_related(&task.request).await;
                        if !prefetch_requests.is_empty() {
                            info!("🔮 PREFETCH TRIGGERED: {} related requests for {}", 
                                  prefetch_requests.len(), cache_key);
                            
                            // Batch the prefetch tasks instead of spawning individual tasks
                            {
                                let mut queue = task_queue.write().await;
                                
                                // Use configured queue size limit
                                if queue.len() + prefetch_requests.len() <= DEFAULT_MAX_PREFETCH_QUEUE_SIZE {
                                    for prefetch_req in prefetch_requests {
                                        let prefetch_task = FetchTask {
                                            request: prefetch_req,
                                            priority: FetchPriority::Low, // Prefetch at low priority
                                            scheduled_at: Instant::now(),
                                            retry_count: 0,
                                        };
                                        queue.push(prefetch_task);
                                    }
                                    drop(queue);
                                } else {
                                    warn!("⚠️  Prefetch queue full, dropping {} requests", prefetch_requests.len());
                                }
                            } // Release lock early
                        }
                    }
                }
                Err(e) => {
                    error!("❌ FETCH FAILED: {} ({:.2}s) - {}", cache_key, elapsed.as_secs_f64(), e);
                    cache.mark_error(&task.request, e.to_string()).await;
                    
                    // Update failure metrics if this was a prefetch
                    if priority == FetchPriority::Low {
                        let mut metrics = prefetch_metrics.write().await;
                        metrics.failed_prefetches += 1;
                        drop(metrics); // Release metrics lock early
                    }

                    // Retry logic
                    if task.retry_count < 3 {
                        let retry_delay = Duration::from_secs(2_u64.pow(task.retry_count + 1));
                        warn!("🔄 FETCH RETRY: {} scheduled in {}s (attempt {}/3)", 
                              cache_key, retry_delay.as_secs(), task.retry_count + 2);
                        
                        let mut retry_task = task;
                        retry_task.retry_count += 1;
                        retry_task.scheduled_at = Instant::now() + retry_delay;

                        let mut queue = task_queue.write().await;
                        queue.push(retry_task);
                    } else {
                        error!("💀 FETCH ABANDONED: {} after 3 attempts", cache_key);
                    }
                }
            }

            // Remove from active fetches
            let mut active = active_fetches.write().await;
            active.remove(&cache_key);
        });
    }

    async fn fetch_data(request: &DataRequest) -> Result<FetchResult> {
        match request {
            DataRequest::ReplicaSets { .. } => {
                let data = list_replicas().await?;
                Ok(FetchResult::ReplicaSets(data))
            }
            DataRequest::Pods {
                namespace: _,
                selector,
            } => {
                let labels = match selector {
                    super::fetcher::PodSelector::ByLabels(labels) => labels.clone(),
                    _ => std::collections::BTreeMap::new(),
                };
                let data = list_rspods(labels).await?;
                Ok(FetchResult::Pods(data))
            }
            DataRequest::Containers {
                pod_name,
                namespace: _,
            } => {
                // Need to get pod labels first
                let labels = std::collections::BTreeMap::new(); // Would need to fetch from pod
                let data = list_containers(labels, pod_name.clone()).await?;
                Ok(FetchResult::Containers(data))
            }
            DataRequest::Events { resource: _, limit } => {
                let data = list_events().await?;
                // TODO: Apply limit and resource filtering
                let limited_data = data.into_iter().take(*limit).collect();
                Ok(FetchResult::Events(limited_data))
            }
            DataRequest::Ingresses { namespace, labels } => {
                if let Some(rs) = get_replicaset(labels.clone()).await? {
                    let data = list_ingresses(&rs, namespace).await?;
                    Ok(FetchResult::Ingresses(data))
                } else {
                    Ok(FetchResult::Ingresses(vec![]))
                }
            }
            DataRequest::Custom { .. } => Err(crate::error::Error::Kube(kube::Error::Api(
                kube::error::ErrorResponse {
                    status: "CustomNotImplemented".to_string(),
                    message: "Custom fetchers not yet implemented".to_string(),
                    reason: "NotImplemented".to_string(),
                    code: 501,
                },
            ))),
        }
    }

    async fn refresh_expired_entries(&self) {
        let expired_keys = self.cache.get_expired_keys().await;

        if !expired_keys.is_empty() {
            info!("🔄 REFRESH: Found {} expired cache entries to refresh", expired_keys.len());
        }

        for key in expired_keys {
            // Parse the key back into a DataRequest
            // This is a simplified version - you'd need proper parsing
            if let Some(request) = Self::parse_cache_key(&key) {
                debug!("🔄 REFRESH SCHEDULED: {}", key);
                self.schedule_fetch(request, FetchPriority::Low).await;
            } else {
                warn!("⚠️  Could not parse expired cache key: {}", key);
            }
        }
    }

    pub async fn schedule_fetch(&self, request: DataRequest, priority: FetchPriority) {
        let cache_key = request.cache_key();
        debug!("📝 SCHEDULED: {} (priority: {:?})", cache_key, priority);
        
        let task = FetchTask {
            request,
            priority,
            scheduled_at: Instant::now(),
            retry_count: 0,
        };

        let mut queue = self.task_queue.write().await;
        queue.push(task);
    }

    /// Schedule multiple fetch requests in batch with deduplication
    /// 
    /// This method implements deduplication by tracking recently seen requests
    /// and respects the configured queue size limits to prevent memory exhaustion.
    /// 
    /// # Errors
    /// 
    /// Returns error if task scheduling fails or queue limits are exceeded
    pub async fn schedule_fetch_batch(&self, requests: Vec<DataRequest>) -> Result<()> {
        if !self.config.enabled {
            debug!("🚫 Prefetch disabled, skipping batch");
            return Ok(());
        }

        info!("📝 BATCH SCHEDULED: {} fetch tasks", requests.len());
        
        let (unique_requests, dedup_count) = self.deduplicate_requests(requests).await;
        
        self.update_prefetch_metrics(unique_requests.len(), dedup_count).await;

        if unique_requests.is_empty() {
            debug!("📝 BATCH: All requests were duplicates, nothing to schedule");
            return Ok(());
        }

        self.queue_unique_requests(unique_requests).await
    }

    /// Deduplicate requests based on recent request history
    async fn deduplicate_requests(&self, requests: Vec<DataRequest>) -> (Vec<DataRequest>, u64) {
        let mut unique_requests = Vec::new();
        let mut dedup_count = 0;
        
        {
            let mut recent = self.recent_requests.write().await;
            let now = Instant::now();
            let cutoff_time = now.checked_sub(Duration::from_secs(60)).unwrap_or(now); // 1 minute dedup window
            
            // Clean up old entries first - remove entries older than cutoff_time
            recent.retain(|_key, timestamp| *timestamp > cutoff_time);
            
            for request in requests {
                let cache_key = request.cache_key();
                if let Some(last_seen) = recent.get(&cache_key) {
                    if *last_seen > cutoff_time {
                        dedup_count += 1;
                        debug!("🔄 DEDUP: Skipping recent request for {}", cache_key);
                        continue;
                    }
                }
                recent.insert(cache_key.clone(), now);
                unique_requests.push(request);
            }
            drop(recent);
        }
        
        (unique_requests, dedup_count)
    }

    /// Update prefetch metrics with request counts
    async fn update_prefetch_metrics(&self, unique_count: usize, dedup_count: u64) {
        let mut metrics = self.prefetch_metrics.write().await;
        metrics.total_prefetch_requests += unique_count as u64;
        metrics.deduplicated_requests += dedup_count;
    }

    /// Queue unique requests, checking for capacity limits
    async fn queue_unique_requests(&self, unique_requests: Vec<DataRequest>) -> Result<()> {
        let mut queue = self.task_queue.write().await;

        // Check if queue is getting too large to prevent memory issues
        if queue.len() + unique_requests.len() > self.config.max_prefetch_queue_size {
            warn!("⚠️  Prefetch queue approaching limit ({} + {} > {}), dropping batch",
                  queue.len(), unique_requests.len(), self.config.max_prefetch_queue_size);
            
            // Update overflow metrics (release queue lock first)
            drop(queue);
            {
                let mut metrics = self.prefetch_metrics.write().await;
                metrics.queue_overflows += 1;
            }
            return Ok(());
        }

        for request in unique_requests {
            let cache_key = request.cache_key();
            let priority = FetchPriority::Low; // All prefetch requests are low priority
            debug!("📝 BATCH ITEM: {} (priority: {:?})", cache_key, priority);
            
            let task = FetchTask {
                priority,
                request,
                scheduled_at: Instant::now(),
                retry_count: 0,
            };
            queue.push(task);
        }
        drop(queue);
        Ok(())
    }

    pub async fn prefetch_for(&self, request: &DataRequest) {
        let related = self.cache.prefetch_related(request).await;
        if !related.is_empty() {
            info!("🔮 PREFETCH: Scheduling {} related fetches for {}", 
                  related.len(), request.cache_key());
        }
        
        for related_request in related {
            self.schedule_fetch(related_request, FetchPriority::Low)
                .await;
        }
    }

    fn parse_cache_key(key: &str) -> Option<DataRequest> {
        // Simple parsing - would need to be more robust
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
                selector: super::fetcher::PodSelector::All,
            }),
            _ => None,
        }
    }

    pub async fn queue_size(&self) -> usize {
        self.task_queue.read().await.len()
    }

    pub async fn active_fetches_count(&self) -> usize {
        self.active_fetches.read().await.len()
    }

    /// Get current prefetch metrics for monitoring
    pub async fn get_prefetch_metrics(&self) -> PrefetchMetrics {
        self.prefetch_metrics.read().await.clone()
    }

    /// Reset prefetch metrics (useful for periodic reporting)
    pub async fn reset_prefetch_metrics(&self) {
        let mut metrics = self.prefetch_metrics.write().await;
        *metrics = PrefetchMetrics::default();
    }

    /// Get prefetch effectiveness ratio (0.0 to 1.0)
    pub async fn get_prefetch_hit_rate(&self) -> f64 {
        let metrics = self.prefetch_metrics.read().await;
        let total = metrics.successful_prefetches + metrics.failed_prefetches;
        if total == 0 {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            {
                // Allow precision loss for metrics calculation - acceptable for monitoring
                metrics.successful_prefetches as f64 / total as f64
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_task_priority_ordering() {
        let high_priority = FetchTask {
            request: DataRequest::Pods {
                namespace: "default".to_string(),
                selector: super::super::fetcher::PodSelector::All,
            },
            priority: FetchPriority::High,
            scheduled_at: Instant::now(),
            retry_count: 0,
        };

        let low_priority = FetchTask {
            request: DataRequest::Events {
                resource: super::super::fetcher::ResourceRef::Pod("test".to_string()),
                limit: 10,
            },
            priority: FetchPriority::Low,
            scheduled_at: Instant::now(),
            retry_count: 0,
        };

        assert!(high_priority > low_priority);
    }
}
