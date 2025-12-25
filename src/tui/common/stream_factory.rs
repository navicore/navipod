use crate::cache_manager;
use crate::k8s::cache::{DataRequest, FetchResult};
use crate::tui::stream::Message;
use futures::Stream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_stream::wrappers::ReceiverStream;
use tracing::debug;

/// Configuration for creating cache-based streams
pub struct CacheStreamConfig {
    pub subscription_pattern: String,
    pub poll_interval_ms: u64,
    pub channel_size: usize,
}

impl Default for CacheStreamConfig {
    fn default() -> Self {
        Self {
            subscription_pattern: "*".to_string(),
            poll_interval_ms: 1000,
            channel_size: 100,
        }
    }
}

/// Factory for creating standardized data streams
pub struct StreamFactory;

impl StreamFactory {
    /// Creates an empty stream (for apps that don't need live data updates)
    pub fn empty() -> impl Stream<Item = Message> {
        futures::stream::empty()
    }

    /// Creates a cache-based stream with subscription and polling fallback
    #[allow(clippy::type_complexity)]
    pub fn cache_based<T, F, P>(
        request: DataRequest,
        config: CacheStreamConfig,
        should_stop: Arc<AtomicBool>,
        initial_data: Vec<T>,
        extractor: F,
        message_constructor: P,
        prefetch_trigger: Option<fn(&Vec<T>, &str) -> futures::future::BoxFuture<'static, ()>>,
    ) -> impl Stream<Item = Message>
    where
        T: Clone + PartialEq + Send + 'static,
        F: Fn(FetchResult) -> Option<Vec<T>> + Send + 'static,
        P: Fn(Vec<T>) -> Message + Send + 'static,
    {
        let (tx, rx) = mpsc::channel(config.channel_size);

        tokio::spawn(async move {
            let cache = cache_manager::get_cache_or_default();

            // Subscribe to cache updates
            let (sub_id, mut cache_rx) = cache
                .subscription_manager
                .subscribe(config.subscription_pattern.clone())
                .await;

            // Start with cached data if available
            if let Some(fetch_result) = cache.get(&request).await {
                if let Some(cached_items) = extractor(fetch_result) {
                    if !cached_items.is_empty() && cached_items != initial_data {
                        // Trigger prefetch if configured
                        if let Some(prefetch_fn) = prefetch_trigger.as_ref() {
                            prefetch_fn(&cached_items, "INITIAL").await;
                        }

                        if tx.send(message_constructor(cached_items)).await.is_err() {
                            cache.subscription_manager.unsubscribe(&sub_id).await;
                            return;
                        }
                    }
                }
            }

            // Listen for cache updates or fallback to direct polling
            while !should_stop.load(Ordering::Relaxed) {
                tokio::select! {
                    // Try to get updates from cache first
                    update = cache_rx.recv() => {
                        if let Some(data_update) = update {
                            // Extract data from the update using a generic approach
                            // This would need to be expanded based on the DataUpdate enum
                            debug!("Received cache update: {:?}", data_update);
                            // For now, we'll trigger a cache check since we can't easily
                            // extract the data generically from DataUpdate
                            if let Some(fetch_result) = cache.get(&request).await {
                                if let Some(new_items) = extractor(fetch_result) {
                                    if !new_items.is_empty() && new_items != initial_data {
                                        // Trigger prefetch if configured
                                        if let Some(prefetch_fn) = prefetch_trigger.as_ref() {
                                            prefetch_fn(&new_items, "UPDATE").await;
                                        }

                                        if tx.send(message_constructor(new_items)).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Fallback: check cache periodically and refresh if needed
                    () = sleep(Duration::from_millis(config.poll_interval_ms)) => {
                        if let Some(fetch_result) = cache.get(&request).await {
                            if let Some(cached_items) = extractor(fetch_result) {
                                debug!("⚡ Using cached data ({} items)", cached_items.len());

                                // Trigger prefetch if configured
                                if let Some(prefetch_fn) = prefetch_trigger.as_ref() {
                                    prefetch_fn(&cached_items, "CACHED").await;
                                }

                                if !cached_items.is_empty() && cached_items != initial_data && tx.send(message_constructor(cached_items)).await.is_err() {
                                    break;
                                }
                            }
                        } else {
                            // Cache miss - try stale data while background fetcher works
                            if let Some(fetch_result) = cache.get_or_mark_stale(&request).await {
                                if let Some(stale_items) = extractor(fetch_result) {
                                    if !stale_items.is_empty() && stale_items != initial_data && tx.send(message_constructor(stale_items)).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            cache.subscription_manager.unsubscribe(&sub_id).await;
        });

        ReceiverStream::new(rx)
    }

    /// Creates a direct polling stream (for APIs without caching)
    pub fn polling_based<T, F, P>(
        poll_interval_ms: u64,
        should_stop: Arc<AtomicBool>,
        initial_data: Vec<T>,
        data_fetcher: F,
        message_constructor: P,
    ) -> impl Stream<Item = Message>
    where
        T: Clone + PartialEq + Send + 'static,
        F: Fn() -> futures::future::BoxFuture<
                'static,
                Result<Vec<T>, Box<dyn std::error::Error + Send>>,
            > + Send
            + 'static,
        P: Fn(Vec<T>) -> Message + Send + 'static,
    {
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            while !should_stop.load(Ordering::Relaxed) {
                match data_fetcher().await {
                    Ok(data) => {
                        if !data.is_empty()
                            && data != initial_data
                            && tx.send(message_constructor(data)).await.is_err()
                        {
                            break;
                        }
                        sleep(Duration::from_millis(poll_interval_ms)).await;
                    }
                    Err(e) => {
                        debug!("Error fetching data: {}", e);
                        break;
                    }
                }
                sleep(Duration::from_millis(poll_interval_ms)).await;
            }
        });

        ReceiverStream::new(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_stream_creation() {
        // Test that empty stream can be created
        let _stream = StreamFactory::empty();
    }

    #[test]
    fn test_config_defaults() {
        let config = CacheStreamConfig::default();
        assert_eq!(config.subscription_pattern, "*");
        assert_eq!(config.poll_interval_ms, 1000);
        assert_eq!(config.channel_size, 100);
    }
}
