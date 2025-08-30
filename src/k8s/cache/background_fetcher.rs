use std::collections::{BinaryHeap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tokio::time::sleep;
use tracing::{debug, error, info};
use crate::error::Result;
use super::data_cache::K8sDataCache;
use super::fetcher::{DataRequest, FetchPriority, FetchResult};

// Import existing fetching functions
use crate::k8s::rs::list_replicas;
use crate::k8s::pods::list_rspods;
use crate::k8s::containers::list as list_containers;
use crate::k8s::events::list_all as list_events;
use crate::k8s::rs_ingress::list_ingresses;
use crate::k8s::rs::get_replicaset;

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
}

impl BackgroundFetcher {
    #[must_use] 
    pub fn new(cache: Arc<K8sDataCache>, max_concurrent: usize) -> Self {
        Self {
            cache,
            task_queue: Arc::new(RwLock::new(BinaryHeap::new())),
            active_fetches: Arc::new(RwLock::new(HashSet::new())),
            max_concurrent_fetches: max_concurrent,
            shutdown_tx: None,
        }
    }

    #[must_use]
    pub fn start(mut self) -> mpsc::Sender<()> {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx.clone());
        
        let fetcher = Arc::new(self);
        
        // Spawn the main fetch loop
        let fetcher_clone = fetcher.clone();
        tokio::spawn(async move {
            fetcher_clone.run_fetch_loop(shutdown_rx).await;
        });
        
        // Spawn the refresh loop for expired entries
        let fetcher_clone = fetcher;
        let (_refresh_shutdown_tx, refresh_shutdown_rx) = mpsc::channel(1);
        tokio::spawn(async move {
            fetcher_clone.run_refresh_loop(refresh_shutdown_rx).await;
        });
        
        shutdown_tx
    }

    async fn run_fetch_loop(&self, mut shutdown_rx: mpsc::Receiver<()>) {
        info!("Background fetcher started");
        
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("Background fetcher shutting down");
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
        info!("Cache refresh loop started");
        
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("Refresh loop shutting down");
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
                return;
            }
            active.insert(cache_key.clone());
        }
        
        let cache = self.cache.clone();
        let active_fetches = self.active_fetches.clone();
        let task_queue = self.task_queue.clone();
        
        tokio::spawn(async move {
            debug!("Fetching data for: {}", cache_key);
            
            // Mark as fetching in cache
            cache.mark_fetching(&task.request).await;
            
            let result = Self::fetch_data(&task.request).await;
            
            match result {
                Ok(data) => {
                    if let Err(e) = cache.put(&task.request, data).await {
                        error!("Failed to cache data for {}: {}", cache_key, e);
                    }
                    debug!("Successfully cached data for: {}", cache_key);
                }
                Err(e) => {
                    error!("Failed to fetch data for {}: {}", cache_key, e);
                    cache.mark_error(&task.request, e.to_string()).await;
                    
                    // Retry logic
                    if task.retry_count < 3 {
                        let mut retry_task = task;
                        retry_task.retry_count += 1;
                        retry_task.scheduled_at = Instant::now() + Duration::from_secs(2_u64.pow(retry_task.retry_count));
                        
                        let mut queue = task_queue.write().await;
                        queue.push(retry_task);
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
            DataRequest::Pods { namespace: _, selector } => {
                let labels = match selector {
                    super::fetcher::PodSelector::ByLabels(labels) => labels.clone(),
                    _ => std::collections::BTreeMap::new(),
                };
                let data = list_rspods(labels).await?;
                Ok(FetchResult::Pods(data))
            }
            DataRequest::Containers { pod_name, namespace: _ } => {
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
            DataRequest::Custom { .. } => {
                Err(crate::error::Error::Kube(kube::Error::Api(kube::error::ErrorResponse {
                    status: "CustomNotImplemented".to_string(),
                    message: "Custom fetchers not yet implemented".to_string(),
                    reason: "NotImplemented".to_string(),
                    code: 501,
                })))
            }
        }
    }

    async fn refresh_expired_entries(&self) {
        let expired_keys = self.cache.get_expired_keys().await;
        
        for key in expired_keys {
            // Parse the key back into a DataRequest
            // This is a simplified version - you'd need proper parsing
            if let Some(request) = Self::parse_cache_key(&key) {
                self.schedule_fetch(request, FetchPriority::Low).await;
            }
        }
    }

    pub async fn schedule_fetch(&self, request: DataRequest, priority: FetchPriority) {
        let task = FetchTask {
            request,
            priority,
            scheduled_at: Instant::now(),
            retry_count: 0,
        };
        
        let mut queue = self.task_queue.write().await;
        queue.push(task);
    }

    pub async fn schedule_fetch_batch(&self, requests: Vec<DataRequest>) {
        let mut queue = self.task_queue.write().await;
        
        for request in requests {
            let task = FetchTask {
                priority: request.priority(),
                request,
                scheduled_at: Instant::now(),
                retry_count: 0,
            };
            queue.push(task);
        }
    }

    pub async fn prefetch_for(&self, request: &DataRequest) {
        let related = self.cache.prefetch_related(request).await;
        for related_request in related {
            self.schedule_fetch(related_request, FetchPriority::Low).await;
        }
    }

    fn parse_cache_key(key: &str) -> Option<DataRequest> {
        // Simple parsing - would need to be more robust
        let parts: Vec<&str> = key.split(':').collect();
        
        match *(parts.first()?) {
            "rs" => Some(DataRequest::ReplicaSets {
                namespace: if parts.get(1)? == &"all" { None } else { Some(parts[1].to_string()) },
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