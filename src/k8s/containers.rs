#![allow(clippy::cognitive_complexity)] // Some functions handle complex k8s data

use crate::cache_manager::get_current_namespace_or_default;
use crate::error::Result;
use crate::k8s::client_manager::{get_client, refresh_client, should_refresh_client};
use crate::k8s::utils::format_label_selector;
use crate::tui::data::{Container, ContainerEnvVar, ContainerMount, ContainerProbe, LogRec};
use k8s_openapi::api::core::v1::{ContainerPort, Pod, Probe};
use kube::{
    ResourceExt,
    api::{Api, ListParams, LogParams, ObjectList},
};
use std::collections::BTreeMap;
use tracing::{debug, warn};
use regex::Regex;
use std::sync::OnceLock;

/// Parse a log line with RFC3339 timestamp and extract components
/// Kubernetes logs often come in format: "2025-01-11T15:30:45.123456789Z message here"
fn parse_log_line(line: &str) -> LogRec {
    static TIMESTAMP_REGEX: OnceLock<Regex> = OnceLock::new();
    
    let regex = TIMESTAMP_REGEX.get_or_init(|| {
        // Match RFC3339 timestamp at start of line, capture the rest as message
        Regex::new(r"^([0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(?:\.[0-9]+)?Z?)\s+(.*)$")
            .unwrap_or_else(|e| panic!("Invalid timestamp regex: {e}"))
    });
    
    regex.captures(line).map_or_else(
        || {
            // No timestamp found, treat entire line as message
            let level = extract_log_level(line);
            LogRec {
                datetime: String::new(),
                level,
                message: line.to_string(),
            }
        },
        |captures| {
            let timestamp = captures.get(1).map_or("", |m| m.as_str());
            let message = captures.get(2).map_or(line, |m| m.as_str());
            
            // Try to extract log level from the message using common patterns
            let level = extract_log_level(message);
            
            LogRec {
                datetime: timestamp.to_string(),
                level,
                message: message.to_string(),
            }
        }
    )
}

/// Extract log level from message content using common patterns
fn extract_log_level(message: &str) -> String {
    static LEVEL_REGEX: OnceLock<Regex> = OnceLock::new();
    
    let regex = LEVEL_REGEX.get_or_init(|| {
        // Match common log level patterns: INFO, DEBUG, WARN, ERROR, etc.
        // Case insensitive, word boundaries, and optional brackets/colons
        Regex::new(r"(?i)\b(trace|debug|info|warn|warning|error|err|fatal|panic)\b")
            .unwrap_or_else(|e| panic!("Invalid log level regex: {e}"))
    });
    
    regex.captures(message).map_or_else(
        String::new, // No recognizable log level
        |captures| captures.get(1).map_or("", |m| m.as_str()).to_uppercase()
    )
}

fn format_ports(ports: Option<Vec<ContainerPort>>) -> String {
    ports.map_or_else(
        || "no ports declaired".to_string(),
        |ports| {
            ports
                .iter()
                .map(|p| {
                    let port_name = p.name.as_deref().unwrap_or("unnamed"); // Use "unnamed" or any default string if name is None
                    format!("{}:{}", port_name, p.container_port)
                })
                .collect::<Vec<_>>()
                .join(", ")
        },
    )
}

/// Extract probe configuration from a Kubernetes probe specification
fn extract_probe_info(probe: &Probe, probe_type: &str) -> ContainerProbe {
    let (handler_type, details) = probe.http_get.as_ref().map_or_else(|| {
        probe.tcp_socket.as_ref().map_or_else(|| {
            probe.exec.as_ref().map_or_else(|| {
                (
                    "Unknown".to_string(),
                    "No handler specified".to_string()
                )
            }, |exec| {
                let command = exec.command.as_ref().map_or_else(|| "No command specified".to_string(), |cmd| cmd.join(" "));
                (
                    "Exec".to_string(),
                    format!("Run: {command}")
                )
            })
        }, |tcp_socket| {
            let port = match &tcp_socket.port {
                k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(port) => port.to_string(),
                k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::String(port) => port.clone(),
            };
            let host = tcp_socket.host.as_deref().unwrap_or("localhost");
            (
                "TCP".to_string(),
                format!("Connect to {host}:{port}")
            )
        })
    }, |http_get| {
        let path = http_get.path.as_deref().unwrap_or("/");
        let port = match &http_get.port {
            k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(port) => port.to_string(),
            k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::String(port) => port.clone(),
        };
        let scheme = http_get.scheme.as_deref().unwrap_or("HTTP");
        let host = http_get.host.as_deref().unwrap_or("localhost");
        (
            "HTTP".to_string(),
            format!("{} {}://{}:{}{}", "GET", scheme.to_lowercase(), host, port, path)
        )
    });

    ContainerProbe {
        probe_type: probe_type.to_string(),
        handler_type,
        details,
        initial_delay: probe.initial_delay_seconds.unwrap_or(0),
        period: probe.period_seconds.unwrap_or(10),
        timeout: probe.timeout_seconds.unwrap_or(1),
        failure_threshold: probe.failure_threshold.unwrap_or(3),
        success_threshold: probe.success_threshold.unwrap_or(1),
    }
}

/// Extract all probes from a Kubernetes container specification
fn extract_container_probes(container: &k8s_openapi::api::core::v1::Container) -> Vec<ContainerProbe> {
    let mut probes = Vec::new();
    
    debug!("Extracting probes for container: {}", container.name);
    
    if let Some(liveness_probe) = &container.liveness_probe {
        let probe = extract_probe_info(liveness_probe, "Liveness");
        debug!("Found liveness probe: {} - {}", probe.handler_type, probe.details);
        probes.push(probe);
    }
    
    if let Some(readiness_probe) = &container.readiness_probe {
        let probe = extract_probe_info(readiness_probe, "Readiness");
        debug!("Found readiness probe: {} - {}", probe.handler_type, probe.details);
        probes.push(probe);
    }
    
    if let Some(startup_probe) = &container.startup_probe {
        let probe = extract_probe_info(startup_probe, "Startup");
        debug!("Found startup probe: {} - {}", probe.handler_type, probe.details);
        probes.push(probe);
    }
    
    // Extract metrics endpoints from pod annotations (this will be added separately with pod metadata)
    
    if probes.is_empty() {
        debug!("No probes found for container: {}", container.name);
    } else {
        debug!("Extracted {} total probes/endpoints for container: {}", probes.len(), container.name);
    }
    
    probes
}

/// Extract resource requirements from container spec
fn extract_container_resources(container: &k8s_openapi::api::core::v1::Container) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    use crate::k8s::resources::{format_cpu, format_memory, parse_cpu, parse_memory};

    let (cpu_request, memory_request) = container.resources.as_ref()
        .and_then(|resources| resources.requests.as_ref())
        .map_or((None, None), |requests| {
            let cpu_req = requests.get("cpu").and_then(|q| parse_cpu(&q.0).map(format_cpu));
            let mem_req = requests.get("memory").and_then(|q| parse_memory(&q.0).map(format_memory));
            (cpu_req, mem_req)
        });

    let (cpu_limit, memory_limit) = container.resources.as_ref()
        .and_then(|resources| resources.limits.as_ref())
        .map_or((None, None), |limits| {
            let cpu_lim = limits.get("cpu").and_then(|q| parse_cpu(&q.0).map(format_cpu));
            let mem_lim = limits.get("memory").and_then(|q| parse_memory(&q.0).map(format_memory));
            (cpu_lim, mem_lim)
        });

    (cpu_request, cpu_limit, memory_request, memory_limit)
}

/// Extract metrics endpoints from pod annotations
fn extract_metrics_from_annotations(annotations: &std::collections::BTreeMap<String, String>, _container_ports: Option<&Vec<k8s_openapi::api::core::v1::ContainerPort>>) -> Vec<ContainerProbe> {
    let mut metrics_probes = Vec::new();
    
    // Check for Prometheus annotations
    if annotations.get("prometheus.io/scrape").map(String::as_str) == Some("true") {
        let metrics_path = annotations.get("prometheus.io/path").cloned().unwrap_or_else(|| "/metrics".to_string());
        let metrics_port = annotations.get("prometheus.io/port")
            .and_then(|p| p.parse::<i32>().ok())
            .unwrap_or(8080);
            
        metrics_probes.push(ContainerProbe {
            probe_type: "Metrics".to_string(),
            handler_type: "HTTP".to_string(),
            details: format!("GET http://localhost:{metrics_port}{metrics_path}"),
            initial_delay: 0,
            period: 10,
            timeout: 5,
            failure_threshold: 1,
            success_threshold: 1,
        });
    }
    
    // Check for navipod.io annotations  
    if annotations.get("navipod.io/metrics-enabled").map(String::as_str) == Some("true") {
        let metrics_path = annotations.get("navipod.io/metrics-path").cloned().unwrap_or_else(|| "/metrics".to_string());
        let metrics_port = annotations.get("navipod.io/metrics-port")
            .and_then(|p| p.parse::<i32>().ok())
            .unwrap_or(8080);
            
        // Only add if not already added by prometheus annotations
        if !metrics_probes.iter().any(|p| p.details.contains(&format!(":{metrics_port}{metrics_path}"))) {
            metrics_probes.push(ContainerProbe {
                probe_type: "Metrics".to_string(),
                handler_type: "HTTP".to_string(),
                details: format!("GET http://localhost:{metrics_port}{metrics_path}"),
                initial_delay: 0,
                period: 10,
                timeout: 5,
                failure_threshold: 1,
                success_threshold: 1,
            });
        }
    }
    
    debug!("Extracted {} metrics endpoints from annotations", metrics_probes.len());
    metrics_probes
}

/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
#[allow(clippy::significant_drop_tightening)]
#[allow(clippy::too_many_lines)]
pub async fn list(selector: BTreeMap<String, String>, pod_name: String) -> Result<Vec<Container>> {
    use crate::k8s::metrics_client::{fetch_pod_metrics, create_metrics_lookup};
    use tracing::debug;

    let mut client = get_client().await?;
    let namespace = get_current_namespace_or_default();
    let label_selector = format_label_selector(&selector);
    let lp = ListParams::default().labels(&label_selector);

    // Try the operation, with one retry on auth error
    // Fetch pod list and metrics in parallel to reduce latency
    let pods_api: Api<Pod> = Api::namespaced((*client).clone(), &namespace);

    let (pod_list_result, pod_metrics_result) = tokio::join!(
        pods_api.list(&lp),
        fetch_pod_metrics((*client).clone(), None)
    );

    let pod_list: ObjectList<Pod> = match pod_list_result {
        Ok(result) => result,
        Err(e) if should_refresh_client(&e) => {
            // Auth error - try refreshing client and retry once
            client = refresh_client().await?;
            // Fetch again in parallel after client refresh
            let retry_pods_api: Api<Pod> = Api::namespaced((*client).clone(), &namespace);
            let (retry_list, retry_metrics) = tokio::join!(
                retry_pods_api.list(&lp),
                fetch_pod_metrics((*client).clone(), None)
            );

            // Use the retry metrics if successful
            let pod_metrics = retry_metrics.unwrap_or_else(|e| {
                debug!("Could not fetch container metrics after retry: {}", e);
                Vec::new()
            });
            let metrics_lookup = create_metrics_lookup(pod_metrics);

            // Process pods with retry results
            let pod_list = retry_list?;
            return process_pod_list(pod_list, pod_name, metrics_lookup);
        }
        Err(e) => return Err(e.into()),
    };

    let pod_metrics = pod_metrics_result.unwrap_or_else(|e| {
        debug!("Could not fetch container metrics: {}", e);
        Vec::new()
    });
    let metrics_lookup = create_metrics_lookup(pod_metrics);

    process_pod_list(pod_list, pod_name, metrics_lookup)
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::unnecessary_wraps)]
fn process_pod_list(
    pod_list: ObjectList<Pod>,
    pod_name: String,
    metrics_lookup: std::collections::HashMap<String, std::collections::HashMap<String, crate::k8s::metrics_client::ContainerMetric>>,
) -> Result<Vec<Container>> {
    use crate::k8s::resources::{format_cpu, format_memory};

    let mut container_vec = Vec::new();

    for pod in pod_list.items {
        let container_statuses = pod
            .status
            .as_ref()
            .and_then(|status| status.container_statuses.clone())
            .unwrap_or_default();

        if let Some(name) = pod.metadata.name {
            if name == pod_name.clone() {
            let container_selectors = pod.metadata.labels;
            if let Some(spec) = pod.spec {
                    for container in spec.containers {
                        // Extract probes first before moving other fields
                        let mut probes = extract_container_probes(&container);

                        // Add metrics endpoints from pod annotations
                        if let Some(ref annotations) = pod.metadata.annotations {
                            let metrics_probes = extract_metrics_from_annotations(annotations, container.ports.as_ref());
                            probes.extend(metrics_probes);
                        }

                        // Extract resource limits and requests
                        let (cpu_request, cpu_limit, memory_request, memory_limit) = extract_container_resources(&container);

                        // Get actual usage from metrics for this container
                        let (cpu_usage, memory_usage) = metrics_lookup.get(&pod_name)
                            .and_then(|m| m.get(&container.name))
                            .map_or((None, None), |container_metrics| {
                                // Record metrics in history for trend visualization
                                crate::cache_manager::record_container_metrics(
                                    &pod_name,
                                    &container.name,
                                    container_metrics.cpu_usage,
                                    container_metrics.memory_usage,
                                );

                                (
                                    container_metrics.cpu_usage.map(format_cpu),
                                    container_metrics.memory_usage.map(format_memory),
                                )
                            });

                        let image = container.image.unwrap_or_else(|| "unknown".to_string());
                        let ports = format_ports(container.ports);
                        let restarts = container_statuses
                            .iter()
                            .find(|cs| cs.name == container.name)
                            .map_or(0, |cs| cs.restart_count)
                            .to_string();

                        let volume_mounts = container.volume_mounts;
                        let mounts: Vec<ContainerMount> = volume_mounts
                            .unwrap_or_else(Vec::new)
                            .into_iter()
                            .map(|vm| ContainerMount {
                                name: vm.name,
                                value: vm.mount_path,
                            })
                            .collect();

                        let env = container.env;
                        let envvars: Vec<ContainerEnvVar> = env
                            .unwrap_or_else(Vec::new)
                            .into_iter()
                            .map(|e| ContainerEnvVar {
                                name: e.name,
                                value: e.value.unwrap_or_default(),
                            })
                            .collect();
                        let c = Container {
                            name: container.name,
                            description: "a pod container".to_string(),
                            restarts,
                            image,
                            ports,
                            mounts,
                            envvars,
                            probes,
                            selectors: container_selectors.clone(),
                            pod_name: pod_name.clone(),
                            cpu_request,
                            cpu_limit,
                            cpu_usage,
                            memory_request,
                            memory_limit,
                            memory_usage,
                        };
                        container_vec.push(c);
                    }

                    if let Some(init_containers) = spec.init_containers {
                        for container in init_containers {
                            // Extract probes first before moving other fields
                            let mut probes = extract_container_probes(&container);

                            // Add metrics endpoints from pod annotations
                            if let Some(ref annotations) = pod.metadata.annotations {
                                let metrics_probes = extract_metrics_from_annotations(annotations, container.ports.as_ref());
                                probes.extend(metrics_probes);
                            }

                            // Extract resource limits and requests
                            let (cpu_request, cpu_limit, memory_request, memory_limit) = extract_container_resources(&container);

                            // Get actual usage from metrics for this init container
                            let (cpu_usage, memory_usage) = metrics_lookup.get(&pod_name)
                                .and_then(|m| m.get(&container.name))
                                .map_or((None, None), |container_metrics| {
                                    // Record metrics in history for trend visualization
                                    crate::cache_manager::record_container_metrics(
                                        &pod_name,
                                        &container.name,
                                        container_metrics.cpu_usage,
                                        container_metrics.memory_usage,
                                    );

                                    (
                                        container_metrics.cpu_usage.map(format_cpu),
                                        container_metrics.memory_usage.map(format_memory),
                                    )
                                });

                            let image = container.image.unwrap_or_else(|| "unknown".to_string());
                            let restarts = container_statuses
                                .iter()
                                .find(|cs| cs.name == container.name)
                                .map_or(0, |cs| cs.restart_count)
                                .to_string();

                            let volume_mounts = container.volume_mounts;
                            let mounts: Vec<ContainerMount> = volume_mounts
                                .unwrap_or_else(Vec::new)
                                .into_iter()
                                .map(|vm| ContainerMount {
                                    name: vm.name,
                                    value: vm.mount_path,
                                })
                                .collect();

                            let env = container.env;
                            let envvars: Vec<ContainerEnvVar> = env
                                .unwrap_or_else(Vec::new)
                                .into_iter()
                                .map(|e| ContainerEnvVar {
                                    name: e.name,
                                    value: e.value.unwrap_or_default(),
                                })
                                .collect();
                            let c = Container {
                                name: container.name,
                                description: "an init container".to_string(), // Distinguish init containers
                                restarts,
                                image,
                                ports: String::new(),
                                mounts,
                                envvars,
                                probes,
                                selectors: container_selectors.clone(),
                                pod_name: pod_name.clone(),
                                cpu_request,
                                cpu_limit,
                                cpu_usage,
                                memory_request,
                                memory_limit,
                                memory_usage,
                            };
                            container_vec.push(c);
                        }
                    }
                }
            }
        }
    }

    Ok(container_vec)
}

/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
#[allow(clippy::significant_drop_tightening)]
pub async fn logs(
    selector: BTreeMap<String, String>,
    pod_name: String,
    container_name: String,
) -> Result<Vec<LogRec>> {
    let mut client = get_client().await?;
    let namespace = get_current_namespace_or_default();
    let label_selector = format_label_selector(&selector);
    let lp = ListParams::default().labels(&label_selector);

    // Try the operation, with one retry on auth error
    let pod_list: ObjectList<Pod> = {
        let pods: Api<Pod> = Api::namespaced((*client).clone(), &namespace);
        match pods.list(&lp).await {
            Ok(result) => result,
            Err(e) if should_refresh_client(&e) => {
                // Auth error - try refreshing client and retry once
                client = refresh_client().await?;
                let pods: Api<Pod> = Api::namespaced((*client).clone(), &namespace);
                pods.list(&lp).await?
            }
            Err(e) => return Err(e.into()),
        }
    };

    let mut log_vec = Vec::new();

    // Find the pod by name
    for pod in pod_list
        .items
        .into_iter()
        .filter(|pod| pod.name_any() == pod_name)
    {
        let log_params = LogParams {
            container: Some(container_name.clone()),
            tail_lines: Some(100), // Adjust based on how many lines you want
            ..Default::default()
        };

        // Fetch logs for the specified container, with retry on auth error
        let pods: Api<Pod> = Api::namespaced((*client).clone(), &namespace);
        let logs = match pods.logs(&pod.name_any(), &log_params).await {
            Ok(result) => result,
            Err(e) if should_refresh_client(&e) => {
                // Auth error - try refreshing client and retry once
                client = refresh_client().await?;
                let pods: Api<Pod> = Api::namespaced((*client).clone(), &namespace);
                pods.logs(&pod.name_any(), &log_params).await?
            }
            Err(e) => return Err(e.into()),
        };

        // Parse and map logs to Vec<LogRec> using our smart parser
        logs.lines().for_each(|line| {
            log_vec.push(parse_log_line(line));
        });
    }
    log_vec.reverse(); // Reverse the order of logs to show the latest logs first

    Ok(log_vec)
}

/// Enhanced logs function with streaming capability (using kube-rs log streaming)
/// 
/// # Arguments
/// * `selector` - Label selector for finding pods
/// * `pod_name` - Name of the pod to stream logs from
/// * `container_name` - Name of the container within the pod
/// * `follow` - Whether to follow/tail the logs in real-time
/// * `tail_lines` - Number of initial lines to fetch (None for all)
/// 
/// # Returns
/// Returns a Vec<LogRec> with parsed log entries
/// 
/// # Errors
/// Will return `Err` if pod cannot be found or logs cannot be streamed
pub async fn logs_enhanced(
    _selector: BTreeMap<String, String>,
    pod_name: String,
    container_name: String,
    follow: bool,
    tail_lines: Option<i64>,
) -> Result<Vec<LogRec>> {
    let mut client = get_client().await?;
    let namespace = get_current_namespace_or_default();

    let log_params = LogParams {
        container: Some(container_name.clone()),
        follow,
        tail_lines,
        timestamps: true, // Include timestamps for better log parsing
        ..Default::default()
    };

    // Get logs with retry on auth error
    let pods: Api<Pod> = Api::namespaced((*client).clone(), &namespace);
    let logs_string = match pods.logs(&pod_name, &log_params).await {
        Ok(logs) => logs,
        Err(e) if should_refresh_client(&e) => {
            // Auth error - try refreshing client and retry once
            debug!("Auth error getting logs, refreshing client and retrying...");
            client = refresh_client().await?;
            let pods: Api<Pod> = Api::namespaced((*client).clone(), &namespace);
            pods.logs(&pod_name, &log_params).await?
        }
        Err(e) => {
            warn!("Failed to get logs for pod {}, container {}: {}", pod_name, container_name, e);
            return Err(e.into());
        }
    };

    debug!("Retrieved logs for pod: {}, container: {}, follow: {}", pod_name, container_name, follow);
    
    // Parse logs into LogRec vector
    let mut log_vec = Vec::new();
    for line in logs_string.lines() {
        if !line.trim().is_empty() {
            log_vec.push(parse_log_line(line.trim()));
        }
    }
    
    // Don't reverse if following (keep chronological order for streaming)
    if !follow {
        log_vec.reverse(); // Reverse the order of logs to show the latest logs first for static logs
    }
    
    debug!("Parsed {} log lines for pod: {}, container: {}", log_vec.len(), pod_name, container_name);
    Ok(log_vec)
}
