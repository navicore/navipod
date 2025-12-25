//! Resource parsing and formatting utilities for Kubernetes CPU and Memory
//!
//! This module provides utilities to parse Kubernetes resource quantities
//! (CPU and Memory) and format them for display in the TUI.

use std::collections::HashMap;
use tracing::warn;

/// Resource usage status based on percentage of limit
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceStatus {
    /// Usage >= 75% of limit (throttling/OOM risk)
    Critical,
    /// Usage between 60-75% of limit
    Warning,
    /// Usage < 60% of limit
    Healthy,
    /// Usage < 20% of request (over-provisioned)
    OverProvisioned,
    /// No limit set or no usage data
    Unknown,
}

/// Parse CPU resource string to millicores
///
/// Examples:
/// - `"100m"` -> `100.0`
/// - `"1"` -> `1000.0`
/// - `"0.5"` -> `500.0`
/// - `"2.5"` -> `2500.0`
///
/// # Arguments
/// * `cpu_str` - Kubernetes CPU quantity string
///
/// # Returns
/// CPU in millicores, or `None` if parsing fails
#[must_use]
pub fn parse_cpu(cpu_str: &str) -> Option<f64> {
    let cpu_str = cpu_str.trim();

    if cpu_str.is_empty() {
        return None;
    }

    // Handle millicores (e.g., "100m")
    if let Some(millis) = cpu_str.strip_suffix('m') {
        return millis.parse::<f64>().ok();
    }

    // Handle cores (e.g., "1", "0.5", "2.5")
    cpu_str.parse::<f64>().ok().map(|cores| cores * 1000.0)
}

/// Parse memory resource string to bytes
///
/// Examples:
/// - `"128Mi"` -> `134217728`
/// - `"1Gi"` -> `1073741824`
/// - `"500M"` -> `500000000`
/// - `"1G"` -> `1000000000`
///
/// # Arguments
/// * `mem_str` - Kubernetes memory quantity string
///
/// # Returns
/// Memory in bytes, or `None` if parsing fails
#[must_use]
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
pub fn parse_memory(mem_str: &str) -> Option<u64> {
    let mem_str = mem_str.trim();

    if mem_str.is_empty() {
        return None;
    }

    // Try to find where the number ends and the unit begins
    let (num_str, unit) = mem_str
        .char_indices()
        .find(|(_, c)| c.is_alphabetic())
        .map_or((mem_str, ""), |(idx, _)| mem_str.split_at(idx));

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
            warn!("Unknown memory unit: {}", unit);
            return None;
        }
    };

    Some((value * multiplier) as u64)
}

/// Format CPU millicores to human-readable string
///
/// Examples:
/// - `100.0` -> `"100m"`
/// - `1000.0` -> `"1"`
/// - `1500.0` -> `"1.5"`
/// - `250.0` -> `"250m"`
///
/// # Arguments
/// * `millicores` - CPU in millicores
///
/// # Returns
/// Formatted CPU string
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn format_cpu(millicores: f64) -> String {
    if millicores < 1000.0 {
        #[allow(clippy::cast_possible_truncation)]
        let millis = millicores as i64;
        format!("{millis}m")
    } else {
        let cores = millicores / 1000.0;
        if cores.fract() == 0.0 {
            #[allow(clippy::cast_possible_truncation)]
            let whole_cores = cores as i64;
            format!("{whole_cores}")
        } else {
            format!("{cores:.1}")
        }
    }
}

/// Format memory bytes to human-readable string
///
/// Examples:
/// - `134217728` -> `"128Mi"`
/// - `1073741824` -> `"1Gi"`
/// - `500000000` -> `"477Mi"`
///
/// # Arguments
/// * `bytes` - Memory in bytes
///
/// # Returns
/// Formatted memory string
#[must_use]
#[allow(clippy::cast_precision_loss)]
#[allow(clippy::cast_possible_truncation)]
pub fn format_memory(bytes: u64) -> String {
    const KI: f64 = 1024.0;
    const MI: f64 = 1024.0 * 1024.0;
    const GI: f64 = 1024.0 * 1024.0 * 1024.0;
    const TI: f64 = 1024.0 * 1024.0 * 1024.0 * 1024.0;

    let bytes_f64 = bytes as f64;

    if bytes_f64 >= TI {
        format!("{:.1}Ti", bytes_f64 / TI)
    } else if bytes_f64 >= GI {
        let gi = bytes_f64 / GI;
        if gi.fract() == 0.0 {
            format!("{}Gi", gi as i64)
        } else {
            format!("{gi:.1}Gi")
        }
    } else if bytes_f64 >= MI {
        let mi = bytes_f64 / MI;
        if mi.fract() == 0.0 {
            format!("{}Mi", mi as i64)
        } else {
            format!("{mi:.0}Mi")
        }
    } else if bytes_f64 >= KI {
        format!("{}Ki", (bytes_f64 / KI) as i64)
    } else {
        format!("{bytes}B")
    }
}

/// Calculate usage percentage compared to limit
///
/// # Arguments
/// * `usage` - Current usage value
/// * `limit` - Limit value
///
/// # Returns
/// Percentage (0.0 - 100.0), or None if limit is 0 or invalid
#[must_use]
pub fn calculate_usage_percent(usage: f64, limit: f64) -> Option<f64> {
    if limit <= 0.0 {
        None
    } else {
        Some((usage / limit) * 100.0)
    }
}

/// Determine resource status based on usage percentage
///
/// # Arguments
/// * `usage_percent` - Usage as percentage of limit
///
/// # Returns
/// Resource status enum
#[must_use]
pub fn determine_status(usage_percent: Option<f64>) -> ResourceStatus {
    match usage_percent {
        Some(pct) if pct >= 75.0 => ResourceStatus::Critical,
        Some(pct) if pct >= 60.0 => ResourceStatus::Warning,
        Some(pct) if pct < 20.0 => ResourceStatus::OverProvisioned,
        Some(_) => ResourceStatus::Healthy,
        None => ResourceStatus::Unknown,
    }
}

/// Container resource metrics
#[derive(Debug, Clone, Default)]
pub struct ContainerResources {
    pub cpu_request: Option<f64>,    // millicores
    pub cpu_limit: Option<f64>,      // millicores
    pub cpu_usage: Option<f64>,      // millicores
    pub memory_request: Option<u64>, // bytes
    pub memory_limit: Option<u64>,   // bytes
    pub memory_usage: Option<u64>,   // bytes
}

impl ContainerResources {
    /// Calculate CPU usage percentage
    #[must_use]
    pub fn cpu_usage_percent(&self) -> Option<f64> {
        match (self.cpu_usage, self.cpu_limit) {
            (Some(usage), Some(limit)) => calculate_usage_percent(usage, limit),
            _ => None,
        }
    }

    /// Calculate memory usage percentage
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn memory_usage_percent(&self) -> Option<f64> {
        match (self.memory_usage, self.memory_limit) {
            (Some(usage), Some(limit)) => calculate_usage_percent(usage as f64, limit as f64),
            _ => None,
        }
    }

    /// Get CPU status
    #[must_use]
    pub fn cpu_status(&self) -> ResourceStatus {
        determine_status(self.cpu_usage_percent())
    }

    /// Get memory status
    #[must_use]
    pub fn memory_status(&self) -> ResourceStatus {
        determine_status(self.memory_usage_percent())
    }

    /// Format CPU for display: "usage/limit [percent%]"
    #[must_use]
    pub fn format_cpu_display(&self) -> String {
        match (self.cpu_usage, self.cpu_limit) {
            (Some(usage), Some(limit)) => {
                let usage_str = format_cpu(usage);
                let limit_str = format_cpu(limit);
                self.cpu_usage_percent().map_or_else(
                    || format!("{usage_str}/{limit_str}"),
                    |pct| format!("{usage_str}/{limit_str} [{pct:.0}%]"),
                )
            }
            (Some(usage), None) => format!("{}/∞", format_cpu(usage)),
            (None, Some(limit)) => format!("?/{}", format_cpu(limit)),
            (None, None) => "N/A".to_string(),
        }
    }

    /// Format memory for display: "usage/limit [percent%]"
    #[must_use]
    pub fn format_memory_display(&self) -> String {
        match (self.memory_usage, self.memory_limit) {
            (Some(usage), Some(limit)) => {
                let usage_str = format_memory(usage);
                let limit_str = format_memory(limit);
                self.memory_usage_percent().map_or_else(
                    || format!("{usage_str}/{limit_str}"),
                    |pct| format!("{usage_str}/{limit_str} [{pct:.0}%]"),
                )
            }
            (Some(usage), None) => format!("{}/∞", format_memory(usage)),
            (None, Some(limit)) => format!("?/{}", format_memory(limit)),
            (None, None) => "N/A".to_string(),
        }
    }
}

/// Aggregate resources from multiple containers
///
/// # Arguments
/// * `containers` - Map of container name to resources
///
/// # Returns
/// Aggregated resources (sum of all containers)
#[must_use]
pub fn aggregate_container_resources<S: std::hash::BuildHasher>(
    containers: &HashMap<String, ContainerResources, S>,
) -> ContainerResources {
    let mut total = ContainerResources::default();

    for resources in containers.values() {
        // Sum CPU
        if let Some(req) = resources.cpu_request {
            total.cpu_request = Some(total.cpu_request.unwrap_or(0.0) + req);
        }
        if let Some(lim) = resources.cpu_limit {
            total.cpu_limit = Some(total.cpu_limit.unwrap_or(0.0) + lim);
        }
        if let Some(usage) = resources.cpu_usage {
            total.cpu_usage = Some(total.cpu_usage.unwrap_or(0.0) + usage);
        }

        // Sum Memory
        if let Some(req) = resources.memory_request {
            total.memory_request = Some(total.memory_request.unwrap_or(0) + req);
        }
        if let Some(lim) = resources.memory_limit {
            total.memory_limit = Some(total.memory_limit.unwrap_or(0) + lim);
        }
        if let Some(usage) = resources.memory_usage {
            total.memory_usage = Some(total.memory_usage.unwrap_or(0) + usage);
        }
    }

    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cpu() {
        assert_eq!(parse_cpu("100m"), Some(100.0));
        assert_eq!(parse_cpu("1"), Some(1000.0));
        assert_eq!(parse_cpu("0.5"), Some(500.0));
        assert_eq!(parse_cpu("2.5"), Some(2500.0));
        assert_eq!(parse_cpu(""), None);
        assert_eq!(parse_cpu("invalid"), None);
    }

    #[test]
    fn test_parse_memory() {
        assert_eq!(parse_memory("128Mi"), Some(134_217_728));
        assert_eq!(parse_memory("1Gi"), Some(1_073_741_824));
        assert_eq!(parse_memory("500M"), Some(500_000_000));
        assert_eq!(parse_memory("1G"), Some(1_000_000_000));
        assert_eq!(parse_memory("1024Ki"), Some(1_048_576));
        assert_eq!(parse_memory(""), None);
    }

    #[test]
    fn test_format_cpu() {
        assert_eq!(format_cpu(100.0), "100m");
        assert_eq!(format_cpu(1000.0), "1");
        assert_eq!(format_cpu(1500.0), "1.5");
        assert_eq!(format_cpu(250.0), "250m");
    }

    #[test]
    fn test_format_memory() {
        assert_eq!(format_memory(134_217_728), "128Mi");
        assert_eq!(format_memory(1_073_741_824), "1Gi");
        assert_eq!(format_memory(500_000_000), "477Mi");
    }

    #[test]
    fn test_calculate_usage_percent() {
        assert_eq!(calculate_usage_percent(50.0, 100.0), Some(50.0));
        assert_eq!(calculate_usage_percent(75.0, 100.0), Some(75.0));
        assert_eq!(calculate_usage_percent(100.0, 100.0), Some(100.0));
        assert_eq!(calculate_usage_percent(50.0, 0.0), None);
    }

    #[test]
    fn test_determine_status() {
        assert_eq!(determine_status(Some(80.0)), ResourceStatus::Critical);
        assert_eq!(determine_status(Some(65.0)), ResourceStatus::Warning);
        assert_eq!(determine_status(Some(50.0)), ResourceStatus::Healthy);
        assert_eq!(
            determine_status(Some(10.0)),
            ResourceStatus::OverProvisioned
        );
        assert_eq!(determine_status(None), ResourceStatus::Unknown);
    }

    #[test]
    fn test_container_resources() {
        let resources = ContainerResources {
            cpu_request: Some(100.0),
            cpu_limit: Some(500.0),
            cpu_usage: Some(250.0),
            memory_request: Some(128 * 1024 * 1024),
            memory_limit: Some(512 * 1024 * 1024),
            memory_usage: Some(256 * 1024 * 1024),
        };

        assert_eq!(resources.cpu_usage_percent(), Some(50.0));
        assert_eq!(resources.memory_usage_percent(), Some(50.0));
        assert_eq!(resources.cpu_status(), ResourceStatus::Healthy);
        assert_eq!(resources.memory_status(), ResourceStatus::Healthy);

        let display = resources.format_cpu_display();
        assert!(display.contains("250m/500m"));
        assert!(display.contains("50%"));
    }

    #[test]
    fn test_aggregate_resources() {
        let mut containers = HashMap::new();

        containers.insert(
            "container1".to_string(),
            ContainerResources {
                cpu_request: Some(100.0),
                cpu_limit: Some(500.0),
                cpu_usage: Some(250.0),
                memory_request: Some(128 * 1024 * 1024),
                memory_limit: Some(512 * 1024 * 1024),
                memory_usage: Some(256 * 1024 * 1024),
            },
        );

        containers.insert(
            "container2".to_string(),
            ContainerResources {
                cpu_request: Some(200.0),
                cpu_limit: Some(1000.0),
                cpu_usage: Some(500.0),
                memory_request: Some(256 * 1024 * 1024),
                memory_limit: Some(1024 * 1024 * 1024),
                memory_usage: Some(512 * 1024 * 1024),
            },
        );

        let total = aggregate_container_resources(&containers);

        assert_eq!(total.cpu_request, Some(300.0));
        assert_eq!(total.cpu_limit, Some(1500.0));
        assert_eq!(total.cpu_usage, Some(750.0));
        assert_eq!(total.memory_request, Some(384 * 1024 * 1024));
        assert_eq!(total.memory_limit, Some(1536 * 1024 * 1024));
        assert_eq!(total.memory_usage, Some(768 * 1024 * 1024));
    }
}
