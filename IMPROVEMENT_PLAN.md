# NaviPod Improvement Plan: Background Data Architecture

## Executive Summary
Transform NaviPod from an on-demand K8s API caller to a proactive data caching system that provides instant navigation with reduced K8s API load.

## Current State Analysis

### Problems Identified
1. **Blocking API Calls**: Each UI navigation triggers synchronous K8s API calls, causing user wait times
2. **Redundant Fetching**: Same data fetched multiple times during navigation
3. **No Prefetching**: Related data (pods → containers → logs) not preloaded
4. **High K8s API Load**: Each user action = multiple API calls
5. **Limited Polling**: Only active view polls for updates (5-second intervals)

### Current Architecture
- Each app (Rs, Pod, Container, etc.) has its own `stream()` method
- Polling only happens for the active view
- Data fetched on-demand when navigating between views
- No shared data cache between views

## Proposed Architecture: Unified Background Data Manager

### Core Components

#### 1. **K8sDataCache** (New Module)
```rust
// src/k8s/cache/mod.rs
struct K8sDataCache {
    replicasets: Arc<RwLock<HashMap<String, CachedData<Vec<Rs>>>>>,
    pods: Arc<RwLock<HashMap<String, CachedData<Vec<Pod>>>>>,
    containers: Arc<RwLock<HashMap<String, CachedData<Vec<Container>>>>>,
    ingresses: Arc<RwLock<HashMap<String, CachedData<Vec<Ingress>>>>>,
    events: Arc<RwLock<HashMap<String, CachedData<Vec<Event>>>>>,
    certificates: Arc<RwLock<HashMap<String, CachedData<Vec<Cert>>>>>,
}

struct CachedData<T> {
    data: T,
    last_updated: Instant,
    ttl: Duration,
    fetch_status: FetchStatus,
}

enum FetchStatus {
    Fresh,
    Stale,
    Fetching,
    Error(String),
}
```

#### 2. **BackgroundFetcher** (New Service)
- Runs continuously in background
- Intelligent prefetching based on user navigation patterns
- Batch API calls for efficiency
- Priority queue for fetch operations

#### 3. **DataSubscription System**
- Views subscribe to data changes
- Push updates to UI components via channels
- Debounce rapid changes

### Implementation Phases

## Phase 1: Core Infrastructure (Week 1-2)

### 1.1 Create Cache Module
- [ ] Implement `K8sDataCache` struct
- [ ] Add TTL-based invalidation
- [ ] Create thread-safe access patterns
- [ ] Implement cache eviction policies

### 1.2 Background Fetcher Service
- [ ] Create fetcher task spawner
- [ ] Implement priority queue for fetch operations
- [ ] Add configurable polling intervals per resource type
- [ ] Create batch fetching for related resources

### 1.3 Data Subscription System
- [ ] Implement pub/sub pattern for cache updates
- [ ] Create subscription manager
- [ ] Add filtering capabilities for subscribers

## Phase 2: Progressive Migration (Week 3-4)

### 2.1 Migrate ReplicaSet View
- [ ] Convert to use cache
- [ ] Implement prefetch for likely navigation targets
- [ ] Add background refresh

### 2.2 Migrate Pod View
- [ ] Use cached pod data
- [ ] Prefetch container data when pods are viewed
- [ ] Prefetch events for selected pods

### 2.3 Migrate Container View
- [ ] Use cached container data
- [ ] Prefetch logs for visible containers
- [ ] Add progressive log loading

## Phase 3: Advanced Features (Week 5-6)

### 3.1 Intelligent Prefetching
- [ ] Learn user navigation patterns
- [ ] Predictive prefetching based on history
- [ ] Resource-aware fetching (don't overload K8s API)

### 3.2 Differential Updates
- [ ] Use K8s watch API for real-time updates
- [ ] Implement resource version tracking
- [ ] Add incremental cache updates

### 3.3 Performance Optimizations
- [ ] Add compression for cached data
- [ ] Implement lazy deserialization
- [ ] Add memory usage limits

## Phase 4: UI Enhancements (Week 7-8)

### 4.1 Loading States
- [ ] Show cache freshness indicators
- [ ] Add refresh animations
- [ ] Display last update timestamps

### 4.2 Offline Mode
- [ ] Work with stale cache when K8s unavailable
- [ ] Show offline indicators
- [ ] Queue actions for when connection returns

### 4.3 Search & Filter Improvements
- [ ] Search across all cached data
- [ ] Add advanced filtering options
- [ ] Implement saved filter sets

## Technical Implementation Details

### Caching Strategy
```rust
// Proposed cache configuration
const CACHE_CONFIG: CacheConfig = CacheConfig {
    replicasets: CachePolicy { ttl: 30s, prefetch: true },
    pods: CachePolicy { ttl: 15s, prefetch: true },
    containers: CachePolicy { ttl: 15s, prefetch: on_view },
    events: CachePolicy { ttl: 60s, prefetch: on_select },
    logs: CachePolicy { ttl: 5s, prefetch: false },
    certificates: CachePolicy { ttl: 300s, prefetch: on_ingress },
};
```

### Prefetch Rules
1. **On App Start**: Fetch all ReplicaSets and top-level resources
2. **On RS Selection**: Prefetch associated Pods and Ingresses
3. **On Pod View**: Prefetch Containers and recent Events
4. **On Container Selection**: Start streaming logs in background
5. **On Ingress View**: Prefetch TLS certificate details

### API Call Batching
- Group related resources in single API calls where possible
- Use label selectors efficiently
- Implement request coalescing for duplicate requests

### Memory Management
- Configurable cache size limits
- LRU eviction for old data
- Compress large text data (logs, events)

## Performance Targets

### Current vs. Target Metrics
| Metric | Current | Target | Improvement |
|--------|---------|--------|-------------|
| View switch time | 1-3s | <100ms | 10-30x |
| K8s API calls/min | 60-120 | 10-20 | 6x reduction |
| Memory usage | ~50MB | ~200MB | Acceptable trade-off |
| Data freshness | Real-time | 5-30s delay | Configurable |

## Migration Strategy

### Step 1: Parallel Implementation
- Build new cache system alongside existing code
- Add feature flag to switch between old/new

### Step 2: Gradual Rollout
- Start with read-only operations
- Test with single view (ReplicaSets)
- Expand to other views progressively

### Step 3: Cleanup
- Remove old direct API calling code
- Optimize cache based on usage patterns
- Add telemetry for cache hit rates

## Configuration

### New Config Options
```toml
[cache]
enabled = true
max_memory_mb = 200
default_ttl_seconds = 30

[prefetch]
enabled = true
aggressive = false
max_concurrent = 5

[polling]
replicasets = 30
pods = 15
events = 60
```

## Testing Strategy

### Unit Tests
- Cache CRUD operations
- TTL expiration
- Concurrent access patterns

### Integration Tests
- Mock K8s API responses
- Test prefetch logic
- Verify subscription updates

### Performance Tests
- Measure cache hit rates
- Monitor memory usage
- Track API call reduction

## Risk Mitigation

### Potential Issues & Solutions
1. **Memory bloat**: Implement strict size limits and eviction
2. **Stale data**: Clear freshness indicators, manual refresh option
3. **K8s API rate limits**: Implement exponential backoff
4. **Complex state management**: Use proven patterns (Arc, RwLock)

## Success Metrics

1. **User Experience**
   - 90% of navigations complete in <100ms
   - Zero blocking UI operations
   - Smooth scrolling with 1000+ items

2. **System Performance**
   - 80% cache hit rate
   - 5x reduction in K8s API calls
   - <200MB memory footprint

3. **Reliability**
   - Graceful degradation when offline
   - No data inconsistencies
   - Automatic recovery from errors

## Next Steps

1. Review and approve plan
2. Set up development branch
3. Create detailed technical design docs
4. Begin Phase 1 implementation
5. Set up performance benchmarking

## Future Enhancements (Post-MVP)

- **Multi-cluster support**: Cache data from multiple clusters
- **Persistent cache**: SQLite for cross-session data
- **Smart alerts**: Notify on important changes
- **Export/reporting**: Generate reports from cached data
- **Plugin system**: Allow custom data sources