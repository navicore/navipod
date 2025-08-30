use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;
use super::fetcher::FetchResult;
use crate::tui::data::{Rs, RsPod, Container, Ingress, ResourceEvent};

#[derive(Debug, Clone)]
pub enum DataUpdate {
    ReplicaSets(Vec<Rs>),
    Pods(Vec<RsPod>),
    Containers(Vec<Container>),
    Events(Vec<ResourceEvent>),
    Ingresses(Vec<Ingress>),
}

impl From<FetchResult> for DataUpdate {
    fn from(result: FetchResult) -> Self {
        match result {
            FetchResult::ReplicaSets(data) => Self::ReplicaSets(data),
            FetchResult::Pods(data) => Self::Pods(data),
            FetchResult::Containers(data) => Self::Containers(data),
            FetchResult::Events(data) => Self::Events(data),
            FetchResult::Ingresses(data) => Self::Ingresses(data),
        }
    }
}

pub struct Subscription {
    pub id: String,
    pub pattern: String,
    pub sender: mpsc::Sender<DataUpdate>,
}

impl Subscription {
    #[must_use]
    pub fn new(pattern: String) -> (Self, mpsc::Receiver<DataUpdate>) {
        let (tx, rx) = mpsc::channel(10);
        let id = Uuid::new_v4().to_string();
        
        (
            Self {
                id,
                pattern,
                sender: tx,
            },
            rx,
        )
    }
}

pub struct SubscriptionManager {
    subscriptions: Arc<RwLock<HashMap<String, Vec<Subscription>>>>,
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SubscriptionManager {
    #[must_use] pub fn new() -> Self {
        Self {
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    #[allow(clippy::significant_drop_tightening)]
    pub async fn subscribe(&self, pattern: String) -> (String, mpsc::Receiver<DataUpdate>) {
        let (subscription, receiver) = Subscription::new(pattern.clone());
        let id = subscription.id.clone();
        
        let mut subs = self.subscriptions.write().await;
        subs.entry(pattern)
            .or_insert_with(Vec::new)
            .push(subscription);
        
        (id, receiver)
    }

    pub async fn unsubscribe(&self, subscription_id: &str) {
        let mut subs = self.subscriptions.write().await;
        
        for (_, subscriptions) in subs.iter_mut() {
            subscriptions.retain(|s| s.id != subscription_id);
        }
        
        // Clean up empty entries
        subs.retain(|_, v| !v.is_empty());
    }

    pub async fn notify(&self, cache_key: &str, data: FetchResult) {
        let subs = self.subscriptions.read().await;
        let update = DataUpdate::from(data);
        
        // Find all subscriptions that match this cache key
        for (pattern, subscriptions) in subs.iter() {
            if Self::pattern_matches(pattern, cache_key) {
                for subscription in subscriptions {
                    // Send update, ignore if receiver dropped
                    let _ = subscription.sender.send(update.clone()).await;
                }
            }
        }
    }

    pub async fn notify_all(&self, updates: Vec<(String, FetchResult)>) {
        for (cache_key, data) in updates {
            self.notify(&cache_key, data).await;
        }
    }

    fn pattern_matches(pattern: &str, cache_key: &str) -> bool {
        // Simple pattern matching for now
        // Could be enhanced with glob patterns or regex
        if pattern == "*" {
            return true;
        }
        
        if pattern == cache_key {
            return true;
        }
        
        // Check if pattern is a prefix (e.g., "pods:*" matches "pods:default:all")
        if let Some(prefix) = pattern.strip_suffix('*') {
            return cache_key.starts_with(prefix);
        }
        
        false
    }

    pub async fn active_subscriptions(&self) -> usize {
        let subs = self.subscriptions.read().await;
        subs.values().map(std::vec::Vec::len).sum()
    }

    pub async fn subscription_patterns(&self) -> Vec<String> {
        let subs = self.subscriptions.read().await;
        subs.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_subscription_pattern_matching() {
        assert!(SubscriptionManager::pattern_matches("*", "anything"));
        assert!(SubscriptionManager::pattern_matches("pods:*", "pods:default:all"));
        assert!(SubscriptionManager::pattern_matches("pods:default", "pods:default"));
        assert!(!SubscriptionManager::pattern_matches("pods:*", "rs:default"));
    }

    #[tokio::test]
    async fn test_subscribe_unsubscribe() {
        let manager = SubscriptionManager::new();
        
        let (id1, _rx1) = manager.subscribe("pods:*".to_string()).await;
        let (id2, _rx2) = manager.subscribe("rs:*".to_string()).await;
        
        assert_eq!(manager.active_subscriptions().await, 2);
        
        manager.unsubscribe(&id1).await;
        assert_eq!(manager.active_subscriptions().await, 1);
        
        manager.unsubscribe(&id2).await;
        assert_eq!(manager.active_subscriptions().await, 0);
    }

    #[tokio::test]
    async fn test_notification() {
        let manager = SubscriptionManager::new();
        
        let (_id, mut rx) = manager.subscribe("pods:*".to_string()).await;
        
        let data = FetchResult::Pods(vec![]);
        manager.notify("pods:default:all", data).await;
        
        // Should receive the notification
        let update = rx.recv().await;
        assert!(update.is_some());
        assert!(matches!(update.unwrap(), DataUpdate::Pods(_)));
    }
}