//! Time-series storage for metrics history
//!
//! Maintains a sliding window of recent metrics (5-10 minutes) for pods and containers
//! to enable trend visualization and analysis.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Maximum number of samples to keep per pod/container
const MAX_SAMPLES: usize = 60; // 60 samples at 5-10 sec intervals = 5-10 minutes

/// Maximum age of samples before pruning
const MAX_AGE: Duration = Duration::from_secs(600); // 10 minutes

/// A single metric sample at a point in time
#[derive(Debug, Clone)]
pub struct MetricSample {
    pub timestamp: Instant,
    pub cpu_millis: Option<f64>,
    pub memory_bytes: Option<u64>,
}

impl MetricSample {
    /// Create a new metric sample
    #[must_use]
    pub fn new(cpu_millis: Option<f64>, memory_bytes: Option<u64>) -> Self {
        Self {
            timestamp: Instant::now(),
            cpu_millis,
            memory_bytes,
        }
    }

    /// Check if this sample is older than the maximum age
    #[must_use]
    pub fn is_expired(&self, max_age: Duration) -> bool {
        self.timestamp.elapsed() > max_age
    }
}

/// Time-series data for a single pod or container
#[derive(Debug, Clone)]
pub struct MetricsTimeSeries {
    samples: VecDeque<MetricSample>,
}

impl Default for MetricsTimeSeries {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsTimeSeries {
    /// Create a new empty time-series
    #[must_use]
    pub fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(MAX_SAMPLES),
        }
    }

    /// Add a new sample to the time-series
    pub fn add_sample(&mut self, cpu_millis: Option<f64>, memory_bytes: Option<u64>) {
        let sample = MetricSample::new(cpu_millis, memory_bytes);
        self.samples.push_back(sample);

        // Prune old samples
        self.prune();
    }

    /// Remove samples older than `MAX_AGE` or beyond `MAX_SAMPLES`
    fn prune(&mut self) {
        // Remove old samples
        while let Some(front) = self.samples.front() {
            if front.is_expired(MAX_AGE) {
                self.samples.pop_front();
            } else {
                break;
            }
        }

        // Keep only MAX_SAMPLES
        while self.samples.len() > MAX_SAMPLES {
            self.samples.pop_front();
        }
    }

    /// Get all samples (most recent first)
    #[must_use]
    pub fn samples(&self) -> Vec<MetricSample> {
        self.samples.iter().rev().cloned().collect()
    }

    /// Get the most recent sample
    #[must_use]
    pub fn latest(&self) -> Option<&MetricSample> {
        self.samples.back()
    }

    /// Get the oldest sample
    #[must_use]
    pub fn oldest(&self) -> Option<&MetricSample> {
        self.samples.front()
    }

    /// Get the number of samples
    #[must_use]
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Check if there are no samples
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Get CPU trend (rising, falling, stable)
    #[must_use]
    pub fn cpu_trend(&self) -> Trend {
        self.calculate_trend(|sample| sample.cpu_millis)
    }

    /// Get memory trend (rising, falling, stable)
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn memory_trend(&self) -> Trend {
        self.calculate_trend(|sample| sample.memory_bytes.map(|b| b as f64))
    }

    /// Calculate trend for a given metric extractor
    #[allow(clippy::cast_precision_loss)]
    fn calculate_trend<F>(&self, extractor: F) -> Trend
    where
        F: Fn(&MetricSample) -> Option<f64>,
    {
        if self.samples.len() < 3 {
            return Trend::Unknown;
        }

        let values: Vec<f64> = self.samples.iter()
            .filter_map(&extractor)
            .collect();

        if values.len() < 3 {
            return Trend::Unknown;
        }

        let first_third: f64 = values.iter().take(values.len() / 3).sum::<f64>() / (values.len() / 3) as f64;
        let last_third: f64 = values.iter().skip(values.len() * 2 / 3).sum::<f64>() / (values.len() / 3) as f64;

        let change_percent = ((last_third - first_third) / first_third) * 100.0;

        if change_percent > 20.0 {
            Trend::Rising
        } else if change_percent < -20.0 {
            Trend::Falling
        } else {
            Trend::Stable
        }
    }

    /// Get CPU values for sparkline rendering (normalized 0-100)
    #[must_use]
    pub fn cpu_sparkline_values(&self, limit_millis: Option<f64>) -> Vec<u8> {
        self.sparkline_values(|sample| sample.cpu_millis, limit_millis)
    }

    /// Get memory values for sparkline rendering (normalized 0-100)
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn memory_sparkline_values(&self, limit_bytes: Option<u64>) -> Vec<u8> {
        self.sparkline_values(
            |sample| sample.memory_bytes.map(|b| b as f64),
            limit_bytes.map(|b| b as f64),
        )
    }

    /// Convert samples to sparkline values (0-100 scale)
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn sparkline_values<F>(&self, extractor: F, limit: Option<f64>) -> Vec<u8>
    where
        F: Fn(&MetricSample) -> Option<f64>,
    {
        let values: Vec<f64> = self.samples.iter()
            .filter_map(&extractor)
            .collect();

        if values.is_empty() {
            return Vec::new();
        }

        // If we have a limit, normalize to percentage of limit
        limit.map_or_else(|| {
            // Otherwise normalize to 0-100 based on min/max in this window
            let min = values.iter().copied().fold(f64::INFINITY, f64::min);
            let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let range = max - min;

            if range < 0.01 {
                // All values essentially the same
                vec![50; values.len()]
            } else {
                values.iter()
                    .map(|&v| {
                        let normalized = ((v - min) / range * 100.0).clamp(0.0, 100.0);
                        normalized as u8
                    })
                    .collect()
            }
        }, |limit_val| {
            values.iter()
                .map(|&v| {
                    let pct = (v / limit_val * 100.0).clamp(0.0, 100.0);
                    pct as u8
                })
                .collect()
        })
    }
}

/// Trend direction indicator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trend {
    Rising,
    Falling,
    Stable,
    Unknown,
}

impl Trend {
    /// Get the arrow symbol for this trend
    #[must_use]
    pub const fn arrow(&self) -> &'static str {
        match self {
            Self::Rising => "↗️",
            Self::Falling => "↘️",
            Self::Stable => "→",
            Self::Unknown => "?",
        }
    }
}

/// Storage for all metrics history
#[derive(Debug, Clone, Default)]
pub struct MetricsHistoryStore {
    pod_metrics: HashMap<String, MetricsTimeSeries>,
    container_metrics: HashMap<(String, String), MetricsTimeSeries>, // (pod_name, container_name)
}

impl MetricsHistoryStore {
    /// Create a new metrics history store
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record pod metrics
    pub fn record_pod_metrics(
        &mut self,
        pod_name: &str,
        cpu_millis: Option<f64>,
        memory_bytes: Option<u64>,
    ) {
        self.pod_metrics
            .entry(pod_name.to_string())
            .or_default()
            .add_sample(cpu_millis, memory_bytes);
    }

    /// Record container metrics
    pub fn record_container_metrics(
        &mut self,
        pod_name: &str,
        container_name: &str,
        cpu_millis: Option<f64>,
        memory_bytes: Option<u64>,
    ) {
        self.container_metrics
            .entry((pod_name.to_string(), container_name.to_string()))
            .or_default()
            .add_sample(cpu_millis, memory_bytes);
    }

    /// Get pod metrics history
    #[must_use]
    pub fn get_pod_history(&self, pod_name: &str) -> Option<&MetricsTimeSeries> {
        self.pod_metrics.get(pod_name)
    }

    /// Get container metrics history
    #[must_use]
    pub fn get_container_history(&self, pod_name: &str, container_name: &str) -> Option<&MetricsTimeSeries> {
        self.container_metrics.get(&(pod_name.to_string(), container_name.to_string()))
    }

    /// Prune all old entries
    pub fn prune_all(&mut self) {
        self.pod_metrics.retain(|_, ts| !ts.is_empty());
        self.container_metrics.retain(|_, ts| !ts.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_time_series_basic() {
        let mut ts = MetricsTimeSeries::new();
        assert!(ts.is_empty());

        ts.add_sample(Some(100.0), Some(1024));
        assert_eq!(ts.len(), 1);
        assert_eq!(ts.latest().unwrap().cpu_millis, Some(100.0));
    }

    #[test]
    fn test_metrics_time_series_prune_by_count() {
        let mut ts = MetricsTimeSeries::new();

        // Add more than MAX_SAMPLES
        for i in 0..MAX_SAMPLES + 10 {
            ts.add_sample(Some(i as f64), Some(i as u64));
        }

        assert_eq!(ts.len(), MAX_SAMPLES);
    }

    #[test]
    fn test_trend_detection() {
        // Rising trend - needs more significant change (>20%)
        let mut ts1 = MetricsTimeSeries::new();
        for i in 0..12 {
            ts1.add_sample(Some((i * 20) as f64 + 100.0), None);
        }
        let trend1 = ts1.cpu_trend();
        assert_eq!(trend1, Trend::Rising, "Expected rising trend for values 100, 120, 140...");

        // Stable trend - all same values
        let mut ts2 = MetricsTimeSeries::new();
        for _ in 0..12 {
            ts2.add_sample(Some(100.0), None);
        }
        let trend2 = ts2.cpu_trend();
        assert_eq!(trend2, Trend::Stable, "Expected stable trend for constant values");

        // Falling trend
        let mut ts3 = MetricsTimeSeries::new();
        for i in 0..12 {
            ts3.add_sample(Some(300.0 - (i * 20) as f64), None);
        }
        let trend3 = ts3.cpu_trend();
        assert_eq!(trend3, Trend::Falling, "Expected falling trend for values 300, 280, 260...");
    }

    #[test]
    fn test_sparkline_values() {
        let mut ts = MetricsTimeSeries::new();

        ts.add_sample(Some(100.0), None);
        ts.add_sample(Some(200.0), None);
        ts.add_sample(Some(300.0), None);

        let values = ts.cpu_sparkline_values(Some(400.0));
        assert_eq!(values.len(), 3);
        assert_eq!(values[0], 25); // 100/400 = 25%
        assert_eq!(values[1], 50); // 200/400 = 50%
        assert_eq!(values[2], 75); // 300/400 = 75%
    }

    #[test]
    fn test_metrics_history_store() {
        let mut store = MetricsHistoryStore::new();

        store.record_pod_metrics("pod1", Some(100.0), Some(1024));
        store.record_container_metrics("pod1", "container1", Some(50.0), Some(512));

        assert!(store.get_pod_history("pod1").is_some());
        assert!(store.get_container_history("pod1", "container1").is_some());
        assert!(store.get_pod_history("pod2").is_none());
    }
}
