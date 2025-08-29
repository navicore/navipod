# NaviPod Architecture Guide: Extensible Data Layer

## Design Principles

### 1. **Widget-Agnostic Data Layer**
The cache doesn't know about UI widgets - widgets request data types they need.

### 2. **Self-Documenting Data Requirements**
Each widget declares its data dependencies explicitly.

### 3. **Composable Data Fetchers**
Complex data needs are composed from simple, reusable fetchers.

## Core Architecture for Extensibility

### Data Request Pattern

```rust
// Every widget implements this trait
trait WidgetDataRequirements {
    fn required_data(&self) -> Vec<DataRequest>;
    fn optional_prefetch(&self) -> Vec<DataRequest>;
    fn refresh_interval(&self) -> Option<Duration>;
}

// Data requests are strongly typed
enum DataRequest {
    ReplicaSets { namespace: Option<String>, labels: LabelSelector },
    Pods { namespace: String, selector: PodSelector },
    Containers { pod_name: String, namespace: String },
    Events { resource: ResourceRef, limit: usize },
    Metrics { resource: ResourceRef, window: Duration },
    Custom { fetcher: Box<dyn DataFetcher> },  // For extensions
}
```

### Adding a New Widget: Step-by-Step Example

Let's say we want to add a new "Network Policy Widget" that shows network policies affecting selected pods:

```rust
// 1. Define the data structure
#[derive(Clone, Debug)]
struct NetworkPolicyData {
    name: String,
    namespace: String,
    pod_selector: LabelSelector,
    ingress_rules: Vec<NetworkRule>,
    egress_rules: Vec<NetworkRule>,
}

// 2. Create a fetcher
struct NetworkPolicyFetcher;

impl DataFetcher for NetworkPolicyFetcher {
    type Output = Vec<NetworkPolicyData>;
    
    async fn fetch(&self, params: FetchParams) -> Result<Self::Output> {
        // K8s API call implementation
        let client = get_client().await?;
        let api: Api<NetworkPolicy> = Api::namespaced(client, &params.namespace);
        // ... fetch and transform logic
    }
    
    fn cache_key(&self, params: &FetchParams) -> String {
        format!("netpol:{}:{}", params.namespace, params.selector)
    }
    
    fn ttl(&self) -> Duration {
        Duration::from_secs(60)  // Network policies change less frequently
    }
}

// 3. Register with the cache system
impl DataRegistry {
    fn register_fetchers() {
        self.register("network_policy", Box::new(NetworkPolicyFetcher));
        // Automatically available to all widgets
    }
}

// 4. Create the widget
struct NetworkPolicyWidget {
    selected_pod: Option<String>,
}

impl WidgetDataRequirements for NetworkPolicyWidget {
    fn required_data(&self) -> Vec<DataRequest> {
        vec![
            DataRequest::Custom {
                fetcher: Box::new(NetworkPolicyFetcher),
            }
        ]
    }
    
    fn optional_prefetch(&self) -> Vec<DataRequest> {
        vec![
            // Prefetch related pods that might be affected
            DataRequest::Pods { 
                namespace: self.namespace.clone(),
                selector: PodSelector::All,
            }
        ]
    }
    
    fn refresh_interval(&self) -> Option<Duration> {
        Some(Duration::from_secs(30))
    }
}
```

## Data Flow Architecture

```
┌─────────────────────────────────────────────────────────┐
│                     UI Layer                             │
├───────────┬───────────┬───────────┬────────────────────┤
│  RS Widget│ Pod Widget│ Log Widget│  New Widget        │
└─────┬─────┴─────┬─────┴─────┬─────┴─────┬──────────────┘
      │           │           │           │
      ▼           ▼           ▼           ▼
┌─────────────────────────────────────────────────────────┐
│              Widget Data Requirements API               │
│  - Declares needed data                                 │
│  - Specifies refresh rates                              │
│  - Defines prefetch hints                               │
└─────────────────────────────────────────────────────────┘
      │
      ▼
┌─────────────────────────────────────────────────────────┐
│                  Data Orchestrator                      │
│  - Deduplicates requests                                │
│  - Manages priorities                                   │
│  - Coordinates batch fetching                           │
└─────────────────────────────────────────────────────────┘
      │
      ▼
┌─────────────────────────────────────────────────────────┐
│                    Cache Layer                          │
│  - TTL management                                       │
│  - Memory limits                                        │
│  - Subscription management                              │
└─────────────────────────────────────────────────────────┘
      │
      ▼
┌─────────────────────────────────────────────────────────┐
│                  Fetcher Registry                       │
├───────────┬───────────┬───────────┬────────────────────┤
│   Core    │  Metrics  │  Custom   │   Plugin           │
│  Fetchers │  Fetcher  │  Fetchers │   Fetchers         │
└───────────┴───────────┴───────────┴────────────────────┘
      │
      ▼
┌─────────────────────────────────────────────────────────┐
│                    K8s API Layer                        │
│  - Connection pooling                                   │
│  - Rate limiting                                        │
│  - Retry logic                                          │
└─────────────────────────────────────────────────────────┘
```

## Developer Guide: Adding New Data Sources

### 1. Simple Data Source (Using Existing K8s Types)

```rust
// For standard K8s resources, just add to the enum
enum DataRequest {
    // ... existing variants ...
    Services { namespace: String, selector: LabelSelector },  // New!
}

// The cache system automatically handles standard resources
```

### 2. Complex Data Source (Custom Logic)

```rust
// For complex data that requires multiple API calls or processing
struct PodMetricsWithHistory;

impl DataFetcher for PodMetricsWithHistory {
    type Output = Vec<PodMetricHistory>;
    
    async fn fetch(&self, params: FetchParams) -> Result<Self::Output> {
        // 1. Fetch current metrics
        let current = fetch_pod_metrics(&params).await?;
        
        // 2. Fetch historical data from metrics server
        let history = fetch_metrics_history(&params).await?;
        
        // 3. Combine and process
        Ok(combine_metrics(current, history))
    }
}
```

### 3. External Data Source (Non-K8s)

```rust
// For integrating external monitoring, logs, etc.
struct PrometheusDataFetcher {
    endpoint: String,
}

impl DataFetcher for PrometheusDataFetcher {
    type Output = TimeSeriesData;
    
    async fn fetch(&self, params: FetchParams) -> Result<Self::Output> {
        // Query Prometheus for pod metrics
        let client = PrometheusClient::new(&self.endpoint);
        client.query(params.into_promql()).await
    }
}
```

## Widget Registration Pattern

```rust
// Each widget module follows this pattern
pub mod my_new_widget {
    // 1. Data structures
    mod data;
    
    // 2. Fetcher implementation
    mod fetcher;
    
    // 3. Widget UI component
    mod ui;
    
    // 4. Registration function
    pub fn register(registry: &mut DataRegistry) {
        registry.add_fetcher("my_data", Box::new(fetcher::MyFetcher));
    }
    
    // 5. Widget factory
    pub fn create() -> Box<dyn Widget> {
        Box::new(ui::MyWidget::new())
    }
}

// Main registration happens at startup
fn initialize_data_layer() -> DataRegistry {
    let mut registry = DataRegistry::new();
    
    // Core widgets
    pods::register(&mut registry);
    containers::register(&mut registry);
    
    // Feature widgets
    #[cfg(feature = "metrics")]
    metrics::register(&mut registry);
    
    // Plugin widgets
    for plugin in discover_plugins() {
        plugin.register(&mut registry);
    }
    
    registry
}
```

## Cache Coordination for Complex Views

When a widget needs data from multiple sources:

```rust
struct PodDetailWidget {
    pod_name: String,
    namespace: String,
}

impl WidgetDataRequirements for PodDetailWidget {
    fn required_data(&self) -> Vec<DataRequest> {
        vec![
            DataRequest::Pods { 
                namespace: self.namespace.clone(),
                selector: PodSelector::ByName(self.pod_name.clone()),
            },
            DataRequest::Events { 
                resource: ResourceRef::Pod(self.pod_name.clone()),
                limit: 50,
            },
            DataRequest::Containers { 
                pod_name: self.pod_name.clone(),
                namespace: self.namespace.clone(),
            },
        ]
    }
    
    // The cache system ensures all data is fetched efficiently
    // and delivered together when ready
}
```

## Subscription and Update Pattern

```rust
// Widgets subscribe to data changes
impl Widget for MyWidget {
    fn on_mount(&mut self, cache: &DataCache) {
        // Subscribe to specific data
        let subscription = cache.subscribe(
            DataRequest::Pods { /* ... */ },
            move |updated_pods| {
                // Handle update
                self.update_display(updated_pods);
            }
        );
        
        self.subscriptions.push(subscription);
    }
    
    fn on_unmount(&mut self, cache: &DataCache) {
        // Automatic cleanup
        for sub in &self.subscriptions {
            cache.unsubscribe(sub);
        }
    }
}
```

## Testing New Data Sources

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::mock_cache;
    
    #[tokio::test]
    async fn test_new_fetcher() {
        // 1. Create mock cache
        let cache = mock_cache();
        
        // 2. Register fetcher
        cache.register_fetcher("test", Box::new(MyFetcher));
        
        // 3. Request data
        let data = cache.get(DataRequest::Custom { /* ... */ }).await;
        
        // 4. Verify
        assert!(data.is_ok());
        assert_eq!(cache.stats().hit_rate(), 0.0); // First fetch
        
        // 5. Test cache hit
        let data2 = cache.get(DataRequest::Custom { /* ... */ }).await;
        assert_eq!(cache.stats().hit_rate(), 1.0);
    }
}
```

## Best Practices

### DO:
- ✅ Create small, focused fetchers
- ✅ Declare all data dependencies upfront
- ✅ Use type-safe data requests
- ✅ Implement proper error handling
- ✅ Add metrics/logging for debugging
- ✅ Write tests for new fetchers

### DON'T:
- ❌ Make K8s API calls directly from widgets
- ❌ Store state in fetchers (they should be stateless)
- ❌ Create circular dependencies between fetchers
- ❌ Ignore TTL recommendations
- ❌ Fetch more data than needed

## Debugging Tools

```rust
// Built-in debugging for data flow
impl DataCache {
    pub fn inspect(&self) -> CacheInspector {
        CacheInspector {
            entries: self.list_all_entries(),
            pending_fetches: self.get_pending_fetches(),
            subscriptions: self.get_active_subscriptions(),
            stats: self.get_stats(),
        }
    }
}

// Use in development
#[cfg(debug_assertions)]
fn debug_data_flow() {
    let inspector = cache.inspect();
    println!("Cache entries: {}", inspector.entries.len());
    println!("Hit rate: {:.2}%", inspector.stats.hit_rate * 100.0);
    println!("Active subscriptions: {}", inspector.subscriptions.len());
}
```

## Migration Guide for Existing Widgets

```rust
// Before: Direct API calls in widget
impl OldWidget {
    async fn load_data(&mut self) {
        let client = create_client().await.unwrap();
        let api: Api<Pod> = Api::namespaced(client, "default");
        self.pods = api.list(&ListParams::default()).await.unwrap();
    }
}

// After: Declare requirements
impl WidgetDataRequirements for NewWidget {
    fn required_data(&self) -> Vec<DataRequest> {
        vec![DataRequest::Pods { 
            namespace: "default".into(),
            selector: PodSelector::All,
        }]
    }
}

// Data automatically provided via subscription
impl Widget for NewWidget {
    fn on_data_update(&mut self, data: DataUpdate) {
        if let DataUpdate::Pods(pods) = data {
            self.pods = pods;
            self.render();
        }
    }
}
```

This architecture ensures that adding new widgets and data sources is straightforward and maintains consistency across the codebase.