//! Kubernetes Metrics Server client
//!
//! This module provides utilities to fetch resource metrics from the
//! Kubernetes Metrics Server API (metrics.k8s.io) using direct API calls.

use crate::error::Result;
use kube::{
    Client,
    api::{Api, ApiResource, DynamicObject, ListParams},
};
use std::collections::HashMap;
use tracing::{debug, warn};

/// Container metrics extracted from `PodMetrics`
#[derive(Debug, Clone)]
pub struct ContainerMetric {
    pub name: String,
    pub cpu_usage: Option<f64>,    // millicores
    pub memory_usage: Option<u64>, // bytes
}

/// Pod metrics with container breakdown
#[derive(Debug, Clone)]
pub struct PodMetric {
    pub pod_name: String,
    pub namespace: String,
    pub containers: Vec<ContainerMetric>,
}

/// Node metrics summary
#[derive(Debug, Clone)]
pub struct NodeMetric {
    pub node_name: String,
    pub cpu_usage: Option<f64>,       // millicores
    pub memory_usage: Option<u64>,    // bytes
    pub cpu_capacity: Option<f64>,    // millicores (from node status, not metrics API)
    pub memory_capacity: Option<u64>, // bytes (from node status, not metrics API)
}

/// Fetch pod metrics from the Kubernetes Metrics Server
///
/// # Arguments
/// * `client` - Kubernetes client
/// * `namespace` - Optional namespace to filter pods (None = all namespaces)
///
/// # Returns
/// Vector of pod metrics, or empty vector if metrics server is not available
///
/// # Errors
/// Returns error only if there's a client/connection issue, not if metrics server is missing
pub async fn fetch_pod_metrics(client: Client, namespace: Option<&str>) -> Result<Vec<PodMetric>> {
    debug!("Fetching pod metrics from metrics server");

    // Define the PodMetrics API resource
    let ar = ApiResource::from_gvk(&kube::core::GroupVersionKind {
        group: "metrics.k8s.io".to_string(),
        version: "v1beta1".to_string(),
        kind: "PodMetrics".to_string(),
    });

    let api: Api<DynamicObject> = namespace.map_or_else(
        || Api::all_with(client.clone(), &ar),
        |ns| Api::namespaced_with(client.clone(), ns, &ar),
    );

    match api.list(&ListParams::default()).await {
        Ok(metrics_list) => {
            debug!(
                "Successfully fetched {} pod metrics",
                metrics_list.items.len()
            );

            let pod_metrics: Vec<PodMetric> = metrics_list
                .items
                .into_iter()
                .filter_map(|pod_metric| {
                    let pod_name = pod_metric.metadata.name.clone()?;
                    let namespace = pod_metric.metadata.namespace.clone()?;

                    // Extract containers array from the dynamic object
                    let containers_data = pod_metric.data.get("containers")?;
                    let containers_array = containers_data.as_array()?;

                    let containers: Vec<ContainerMetric> = containers_array
                        .iter()
                        .filter_map(|container| {
                            let name = container.get("name")?.as_str()?.to_string();
                            let usage = container.get("usage")?;

                            let cpu_str = usage.get("cpu")?.as_str()?;
                            let memory_str = usage.get("memory")?.as_str()?;

                            let cpu_usage = parse_cpu_quantity(cpu_str);
                            let memory_usage = parse_memory_quantity(memory_str);

                            Some(ContainerMetric {
                                name,
                                cpu_usage,
                                memory_usage,
                            })
                        })
                        .collect();

                    Some(PodMetric {
                        pod_name,
                        namespace,
                        containers,
                    })
                })
                .collect();

            debug!("Parsed {} pod metrics successfully", pod_metrics.len());
            Ok(pod_metrics)
        }
        Err(e) => {
            // Check if this is a "not found" error (metrics server not installed)
            let error_message = e.to_string();
            if error_message.contains("NotFound")
                || error_message.contains("metrics.k8s.io")
                || error_message.contains("not found")
                || error_message.contains("404")
            {
                debug!("Metrics server not available, continuing without metrics");
                Ok(Vec::new()) // Return empty vector, not an error
            } else {
                warn!("Error fetching pod metrics: {}", e);
                Err(e.into())
            }
        }
    }
}

/// Fetch node metrics from the Kubernetes Metrics Server
///
/// # Arguments
/// * `client` - Kubernetes client
///
/// # Returns
/// Vector of node metrics, or empty vector if metrics server is not available
///
/// # Errors
/// Returns error only if there's a client/connection issue, not if metrics server is missing
pub async fn fetch_node_metrics(client: Client) -> Result<Vec<NodeMetric>> {
    debug!("Fetching node metrics from metrics server");

    // Define the NodeMetrics API resource
    let ar = ApiResource::from_gvk(&kube::core::GroupVersionKind {
        group: "metrics.k8s.io".to_string(),
        version: "v1beta1".to_string(),
        kind: "NodeMetrics".to_string(),
    });

    let api: Api<DynamicObject> = Api::all_with(client.clone(), &ar);

    match api.list(&ListParams::default()).await {
        Ok(metrics_list) => {
            debug!(
                "Successfully fetched {} node metrics",
                metrics_list.items.len()
            );

            let node_metrics: Vec<NodeMetric> = metrics_list
                .items
                .into_iter()
                .filter_map(|node_metric| {
                    let node_name = node_metric.metadata.name.clone()?;

                    // Extract usage from the dynamic object
                    let usage = node_metric.data.get("usage")?;
                    let cpu_str = usage.get("cpu")?.as_str()?;
                    let memory_str = usage.get("memory")?.as_str()?;

                    let cpu_usage = parse_cpu_quantity(cpu_str);
                    let memory_usage = parse_memory_quantity(memory_str);

                    Some(NodeMetric {
                        node_name,
                        cpu_usage,
                        memory_usage,
                        cpu_capacity: None, // Will be filled from Node API separately
                        memory_capacity: None, // Will be filled from Node API separately
                    })
                })
                .collect();

            debug!("Parsed {} node metrics successfully", node_metrics.len());
            Ok(node_metrics)
        }
        Err(e) => {
            // Check if this is a "not found" error (metrics server not installed)
            let error_message = e.to_string();
            if error_message.contains("NotFound")
                || error_message.contains("metrics.k8s.io")
                || error_message.contains("not found")
                || error_message.contains("404")
            {
                debug!("Metrics server not available, continuing without node metrics");
                Ok(Vec::new()) // Return empty vector, not an error
            } else {
                warn!("Error fetching node metrics: {}", e);
                Err(e.into())
            }
        }
    }
}

/// Create a lookup map of container metrics by pod name
///
/// # Arguments
/// * `pod_metrics` - Vector of pod metrics
///
/// # Returns
/// `HashMap` mapping `pod_name` -> `HashMap(container_name -> ContainerMetric)`
#[must_use]
pub fn create_metrics_lookup(
    pod_metrics: Vec<PodMetric>,
) -> HashMap<String, HashMap<String, ContainerMetric>> {
    let mut lookup: HashMap<String, HashMap<String, ContainerMetric>> = HashMap::new();

    for pod_metric in pod_metrics {
        let mut container_map = HashMap::new();

        for container_metric in pod_metric.containers {
            container_map.insert(container_metric.name.clone(), container_metric);
        }

        lookup.insert(pod_metric.pod_name, container_map);
    }

    lookup
}

/// Parse Kubernetes CPU quantity string to millicores
///
/// The metrics API returns CPU in nanocores format, e.g., "12345678n"
/// We need to convert to millicores for consistency
///
/// # Arguments
/// * `quantity_str` - Kubernetes quantity string from metrics API
///
/// # Returns
/// CPU in millicores, or None if parsing fails
fn parse_cpu_quantity(quantity_str: &str) -> Option<f64> {
    // Handle nanocores (e.g., "12345678n")
    if let Some(nanos_str) = quantity_str.strip_suffix('n') {
        if let Ok(nanos) = nanos_str.parse::<f64>() {
            // Convert nanocores to millicores: nanocores / 1_000_000
            return Some(nanos / 1_000_000.0);
        }
    }

    // Handle microcores (e.g., "12345u")
    if let Some(micros_str) = quantity_str.strip_suffix('u') {
        if let Ok(micros) = micros_str.parse::<f64>() {
            // Convert microcores to millicores: microcores / 1000
            return Some(micros / 1000.0);
        }
    }

    // Handle millicores (e.g., "123m")
    if let Some(millis_str) = quantity_str.strip_suffix('m') {
        return millis_str.parse::<f64>().ok();
    }

    // Handle cores (e.g., "1", "0.5")
    if let Ok(cores) = quantity_str.parse::<f64>() {
        return Some(cores * 1000.0);
    }

    warn!("Failed to parse CPU quantity: {}", quantity_str);
    None
}

/// Parse Kubernetes memory quantity string to bytes
///
/// # Arguments
/// * `quantity_str` - Kubernetes quantity string from metrics API
///
/// # Returns
/// Memory in bytes, or None if parsing fails
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
fn parse_memory_quantity(quantity_str: &str) -> Option<u64> {
    // Try to find where the number ends and the unit begins
    let (num_str, unit) = quantity_str
        .char_indices()
        .find(|(_, c)| c.is_alphabetic())
        .map_or((quantity_str, ""), |(idx, _)| quantity_str.split_at(idx));

    let value = num_str.parse::<f64>().ok()?;

    // Parse unit suffix
    let multiplier = match unit {
        // Binary units (powers of 1024)
        "Ki" => 1024.0,
        "Mi" => 1024.0 * 1024.0,
        "Gi" => 1024.0 * 1024.0 * 1024.0,
        "Ti" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        // Decimal units (powers of 1000)
        "K" | "k" => 1000.0,
        "M" | "m" => 1000.0 * 1000.0,
        "G" | "g" => 1000.0 * 1000.0 * 1000.0,
        "T" | "t" => 1000.0 * 1000.0 * 1000.0 * 1000.0,
        // No unit means bytes
        "" => 1.0,
        _ => {
            warn!("Unknown memory unit in quantity: {}", unit);
            return None;
        }
    };

    Some((value * multiplier) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cpu_quantity() {
        // Nanocores
        assert_eq!(parse_cpu_quantity("12345678n"), Some(12.345_678));
        assert_eq!(parse_cpu_quantity("1000000n"), Some(1.0));

        // Millicores
        assert_eq!(parse_cpu_quantity("100m"), Some(100.0));
        assert_eq!(parse_cpu_quantity("500m"), Some(500.0));

        // Cores
        assert_eq!(parse_cpu_quantity("1"), Some(1000.0));
        assert_eq!(parse_cpu_quantity("2.5"), Some(2500.0));
    }

    #[test]
    fn test_parse_memory_quantity() {
        // Binary units
        assert_eq!(parse_memory_quantity("128Mi"), Some(134_217_728));
        assert_eq!(parse_memory_quantity("1Gi"), Some(1_073_741_824));
        assert_eq!(parse_memory_quantity("1024Ki"), Some(1_048_576));

        // Decimal units
        assert_eq!(parse_memory_quantity("500M"), Some(500_000_000));
        assert_eq!(parse_memory_quantity("1G"), Some(1_000_000_000));

        // Bytes
        assert_eq!(parse_memory_quantity("1024"), Some(1024));
    }

    #[test]
    fn test_create_metrics_lookup() {
        let pod_metrics = vec![
            PodMetric {
                pod_name: "pod1".to_string(),
                namespace: "default".to_string(),
                containers: vec![
                    ContainerMetric {
                        name: "container1".to_string(),
                        cpu_usage: Some(100.0),
                        memory_usage: Some(128 * 1024 * 1024),
                    },
                    ContainerMetric {
                        name: "container2".to_string(),
                        cpu_usage: Some(200.0),
                        memory_usage: Some(256 * 1024 * 1024),
                    },
                ],
            },
            PodMetric {
                pod_name: "pod2".to_string(),
                namespace: "default".to_string(),
                containers: vec![ContainerMetric {
                    name: "container1".to_string(),
                    cpu_usage: Some(50.0),
                    memory_usage: Some(64 * 1024 * 1024),
                }],
            },
        ];

        let lookup = create_metrics_lookup(pod_metrics);

        assert_eq!(lookup.len(), 2);
        assert!(lookup.contains_key("pod1"));
        assert!(lookup.contains_key("pod2"));

        let pod1_containers = lookup.get("pod1").unwrap();
        assert_eq!(pod1_containers.len(), 2);
        assert!(pod1_containers.contains_key("container1"));
        assert!(pod1_containers.contains_key("container2"));

        let container1 = pod1_containers.get("container1").unwrap();
        assert_eq!(container1.cpu_usage, Some(100.0));
    }
}
