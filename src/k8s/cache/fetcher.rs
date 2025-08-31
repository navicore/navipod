use crate::error::Result;
use crate::tui::data::{Container, Ingress, Rs, RsPod};
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct FetchParams {
    pub namespace: Option<String>,
    pub labels: BTreeMap<String, String>,
    pub name: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum DataRequest {
    ReplicaSets {
        namespace: Option<String>,
        labels: BTreeMap<String, String>,
    },
    Pods {
        namespace: String,
        selector: PodSelector,
    },
    Containers {
        pod_name: String,
        namespace: String,
    },
    Events {
        resource: ResourceRef,
        limit: usize,
    },
    Ingresses {
        namespace: String,
        labels: BTreeMap<String, String>,
    },
    Custom {
        fetcher_id: String,
        params: FetchParams,
    },
}

#[derive(Debug, Clone)]
pub enum PodSelector {
    All,
    ByLabels(BTreeMap<String, String>),
    ByName(String),
}

#[derive(Debug, Clone)]
pub enum ResourceRef {
    Pod(String),
    ReplicaSet(String),
    Deployment(String),
    Service(String),
    All, // For fetching all events cluster-wide
}

impl DataRequest {
    #[must_use]
    pub fn cache_key(&self) -> String {
        match self {
            Self::ReplicaSets { namespace, labels } => {
                format!("rs:{}:{labels:?}", namespace.as_deref().unwrap_or("all"))
            }
            Self::Pods {
                namespace,
                selector,
            } => {
                format!("pods:{namespace}:{selector:?}")
            }
            Self::Containers {
                pod_name,
                namespace,
            } => {
                format!("containers:{namespace}:{pod_name}")
            }
            Self::Events { resource, limit } => {
                format!("events:{resource:?}:{limit}")
            }
            Self::Ingresses { namespace, labels } => {
                format!("ingress:{namespace}:{labels:?}")
            }
            Self::Custom { fetcher_id, params } => {
                format!(
                    "custom:{fetcher_id}:{:?}:{:?}",
                    params.namespace, params.labels
                )
            }
        }
    }

    #[must_use]
    pub const fn default_ttl(&self) -> Duration {
        match self {
            // Longer TTL for predictive cache - ReplicaSets change infrequently
            Self::ReplicaSets { .. } | Self::Custom { .. } => Duration::from_secs(300), // 5 minutes
            // Pods change more frequently but still want to avoid cache misses during navigation
            Self::Pods { .. } | Self::Containers { .. } => Duration::from_secs(120), // 2 minutes
            // Events and Ingresses can be cached longer since they're less critical for navigation
            Self::Events { .. } | Self::Ingresses { .. } => Duration::from_secs(180), // 3 minutes
        }
    }

    #[must_use]
    pub const fn priority(&self) -> FetchPriority {
        match self {
            Self::Pods { .. } | Self::Containers { .. } => FetchPriority::High,
            Self::ReplicaSets { .. } | Self::Custom { .. } => FetchPriority::Medium,
            Self::Events { .. } | Self::Ingresses { .. } => FetchPriority::Low,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FetchPriority {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

#[async_trait]
pub trait DataFetcher: Send + Sync {
    type Output: Clone + Send + Sync;

    async fn fetch(&self, params: FetchParams) -> Result<Self::Output>;

    fn cache_key(&self, params: &FetchParams) -> String;

    fn ttl(&self) -> Duration {
        Duration::from_secs(30)
    }

    fn priority(&self) -> FetchPriority {
        FetchPriority::Medium
    }
}

#[derive(Debug)]
pub enum FetchResult {
    ReplicaSets(Vec<Rs>),
    Pods(Vec<RsPod>),
    Containers(Vec<Container>),
    Events(Vec<crate::tui::data::ResourceEvent>),
    Ingresses(Vec<Ingress>),
}

impl Clone for FetchResult {
    fn clone(&self) -> Self {
        match self {
            Self::ReplicaSets(data) => Self::ReplicaSets(data.clone()),
            Self::Pods(data) => Self::Pods(data.clone()),
            Self::Containers(data) => Self::Containers(data.clone()),
            Self::Events(data) => Self::Events(data.clone()),
            Self::Ingresses(data) => Self::Ingresses(data.clone()),
        }
    }
}
