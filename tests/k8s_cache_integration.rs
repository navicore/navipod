/// Integration test that validates the cache against a real K8s cluster
///
/// Run with: cargo test --test `k8s_cache_integration` -- --nocapture
///
/// This test will:
/// 1. Skip if no K8s cluster is available
/// 2. Verify cache is faster than direct API calls
/// 3. Verify background fetcher works with real data
/// 4. Verify data consistency
use navipod::k8s::cache::{
    BackgroundFetcher, DataRequest, FetchPriority, FetchResult, K8sDataCache,
};
use navipod::k8s::client::new as create_client;
use navipod::k8s::rs::list_replicas;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Once;
use std::time::{Duration, Instant};

static INIT: Once = Once::new();

fn init_rustls() {
    INIT.call_once(|| {
        // Initialize rustls provider for tests
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .expect("Failed to install rustls crypto provider");
    });
}

async fn k8s_available() -> bool {
    init_rustls();
    create_client(None).await.is_ok()
}

#[tokio::test]
async fn test_cache_with_real_k8s() {
    // Skip test if no K8s cluster available
    if !k8s_available().await {
        eprintln!("Skipping K8s integration test - no cluster available");
        return;
    }

    let cache = Arc::new(K8sDataCache::new(10));

    // Test 1: Verify cache miss then hit performance
    let request = DataRequest::ReplicaSets {
        namespace: None,
        labels: BTreeMap::new(),
    };

    // Cache miss
    assert!(cache.get(&request).await.is_none());

    // Fetch real data
    let start = Instant::now();
    let real_data = list_replicas().await.expect("Failed to list ReplicaSets");
    let api_time = start.elapsed();

    // Store in cache
    cache
        .put(&request, FetchResult::ReplicaSets(real_data.clone()))
        .await
        .expect("Failed to cache data");

    // Cache hit should be much faster
    let start = Instant::now();
    let cached = cache.get(&request).await;
    let cache_time = start.elapsed();

    assert!(cached.is_some());
    assert!(
        cache_time < api_time / 10,
        "Cache hit ({cache_time:?}) should be at least 10x faster than API call ({api_time:?})"
    );

    // Test 2: Verify data matches
    match cached.unwrap() {
        FetchResult::ReplicaSets(cached_rs) => {
            assert_eq!(cached_rs.len(), real_data.len());
        }
        _ => panic!("Wrong data type returned from cache"),
    }
}

#[tokio::test]
async fn test_background_fetcher_with_k8s() {
    if !k8s_available().await {
        eprintln!("Skipping K8s background fetcher test - no cluster available");
        return;
    }

    let cache = Arc::new(K8sDataCache::new(10));
    let fetcher = BackgroundFetcher::new(cache.clone(), 3);
    let (fetcher, shutdown_tx) = fetcher.start();

    // Schedule a real fetch
    let request = DataRequest::ReplicaSets {
        namespace: Some("default".to_string()),
        labels: BTreeMap::new(),
    };

    fetcher
        .schedule_fetch(request.clone(), FetchPriority::High)
        .await;

    // Wait for background fetch to complete
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Data should be in cache now
    let cached = cache.get(&request).await;
    assert!(
        cached.is_some(),
        "Background fetcher should have populated cache"
    );

    // Cleanup
    drop(shutdown_tx);
}

#[tokio::test]
async fn test_cache_invalidation_and_refresh() {
    if !k8s_available().await {
        eprintln!("Skipping cache invalidation test - no cluster available");
        return;
    }

    let cache = Arc::new(K8sDataCache::new(10));

    let request = DataRequest::ReplicaSets {
        namespace: None,
        labels: BTreeMap::new(),
    };

    // Fetch and cache data
    let data = list_replicas().await.expect("Failed to list ReplicaSets");
    cache
        .put(&request, FetchResult::ReplicaSets(data))
        .await
        .expect("Failed to cache data");

    // Verify cached
    assert!(cache.get(&request).await.is_some());

    // Invalidate
    cache.invalidate(&request).await;

    // Should return None when stale (get only returns fresh data)
    assert!(cache.get(&request).await.is_none());

    // Remove the entry completely to test clean state
    cache.remove(&request).await;
    assert!(cache.get(&request).await.is_none());
}

#[tokio::test]
async fn test_concurrent_k8s_operations() {
    if !k8s_available().await {
        eprintln!("Skipping concurrent operations test - no cluster available");
        return;
    }

    let cache = Arc::new(K8sDataCache::new(50));
    let mut handles = vec![];

    // Spawn multiple tasks doing cache operations
    for i in 0..5 {
        let cache_clone = cache.clone();
        let handle = tokio::spawn(async move {
            let request = DataRequest::ReplicaSets {
                namespace: Some(format!("test-ns-{i}")),
                labels: BTreeMap::new(),
            };

            // Try to fetch real data (might fail for non-existent namespaces)
            if let Ok(data) = list_replicas().await {
                let result = FetchResult::ReplicaSets(data);
                cache_clone
                    .put(&request, result)
                    .await
                    .expect("Cache put failed");

                // Verify we can read it back
                assert!(cache_clone.get(&request).await.is_some());
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        handle.await.expect("Task failed");
    }

    // Check final state
    let stats = cache.stats().await;
    assert!(stats.total_entries > 0);
    assert!(stats.memory_used_bytes > 0);
}
