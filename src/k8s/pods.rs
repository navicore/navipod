use crate::cache_manager::get_current_namespace_or_default;
use crate::error::Result;
use crate::k8s::events::{format_duration, list_events_for_resource, list_k8sevents};
use crate::k8s::metrics_client::{
    ContainerMetric, create_metrics_lookup, fetch_node_metrics, fetch_pod_metrics,
};
use crate::k8s::resources::{format_cpu, format_memory, parse_cpu, parse_memory};
use crate::k8s::utils::format_label_selector;
use crate::tui::data::RsPod;
use chrono::{DateTime, Utc};
use k8s_openapi::api::core::v1::{Event, Node, Pod};
use kube::Api;
use kube::api::ListParams;
use std::collections::{BTreeMap, HashMap};
use tracing::debug;

use super::{USER_AGENT, client::new};

fn calculate_pod_age(pod: &Pod) -> String {
    pod.metadata.creation_timestamp.as_ref().map_or_else(
        || "Unk".to_string(),
        |creation_timestamp| {
            let ts: DateTime<Utc> = DateTime::from_timestamp(
                creation_timestamp.0.as_second(),
                creation_timestamp.0.subsec_nanosecond().cast_unsigned(),
            )
            .unwrap_or_default();
            let now = Utc::now();
            let duration = now.signed_duration_since(ts);
            format_duration(duration)
        },
    )
}

fn get_pod_state(pod: &Pod) -> String {
    // Check if the pod is marked for deletion
    if pod.metadata.deletion_timestamp.is_some() {
        return "Terminating".to_string();
    }

    // Then proceed to check the pod's status as before
    if let Some(status) = &pod.status
        && let Some(phase) = &status.phase
    {
        return match phase.as_str() {
            "Pending" => "Pending".to_string(),
            "Running" => {
                if status.conditions.as_ref().is_some_and(|conds| {
                    conds
                        .iter()
                        .any(|c| c.type_ == "Ready" && c.status == "True")
                }) {
                    "Running".to_string()
                } else {
                    "Starting".to_string() // This might be a more accurate state for non-ready running pods
                }
            }
            "Succeeded" => "Succeeded".to_string(),
            "Failed" => "Failed".to_string(),
            _ => "Unknown".to_string(),
        };
    }

    "Unknown".to_string()
}

/// Build a `node_name -> (cpu_percent, memory_percent)` lookup from nodes + metrics.
fn build_node_info(
    nodes: &[Node],
    node_metrics: &[crate::k8s::metrics_client::NodeMetric],
) -> HashMap<String, (f64, f64)> {
    let mut node_info: HashMap<String, (f64, f64)> = HashMap::new();
    for node in nodes {
        let Some(node_name) = &node.metadata.name else {
            continue;
        };

        let (cpu_capacity, mem_capacity) = node
            .status
            .as_ref()
            .and_then(|status| status.capacity.as_ref())
            .map_or((None, None), |capacity| {
                let cpu = capacity.get("cpu").and_then(|q| parse_cpu(&q.0));
                let mem = capacity.get("memory").and_then(|q| parse_memory(&q.0));
                (cpu, mem)
            });

        let (cpu_usage, mem_usage) = node_metrics
            .iter()
            .find(|nm| nm.node_name == *node_name)
            .map_or((None, None), |nm| (nm.cpu_usage, nm.memory_usage));

        let cpu_pct = match (cpu_usage, cpu_capacity) {
            (Some(usage), Some(capacity)) if capacity > 0.0 => Some((usage / capacity) * 100.0),
            _ => None,
        };
        #[allow(clippy::cast_precision_loss)]
        let mem_pct = match (mem_usage, mem_capacity) {
            (Some(usage), Some(capacity)) if capacity > 0 => {
                Some((usage as f64 / capacity as f64) * 100.0)
            }
            _ => None,
        };

        if let (Some(cpu), Some(mem)) = (cpu_pct, mem_pct) {
            node_info.insert(node_name.clone(), (cpu, mem));
        }
    }
    node_info
}

/// Project a single `Pod` into an `RsPod` row using the supplied auxiliary lookups.
///
/// `kind` is the value written to `RsPod.description` (e.g. `"ReplicaSet"`,
/// `"Unowned"`). Events are filtered to this pod's name inside the helper.
#[allow(clippy::too_many_lines)]
async fn project_pod(
    pod: &Pod,
    kind: &str,
    events: &[Event],
    metrics_lookup: &HashMap<String, HashMap<String, ContainerMetric>>,
    node_info: &HashMap<String, (f64, f64)>,
) -> Result<RsPod> {
    let instance_name = pod
        .metadata
        .name
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    let actual_container_count = pod.status.as_ref().map_or(0, |status| {
        status
            .container_statuses
            .as_ref()
            .map_or(0, |container_statuses| {
                container_statuses.iter().filter(|cs| cs.ready).count()
            })
    });

    let desired_container_count = pod.spec.as_ref().map_or(0, |spec| spec.containers.len());

    let age = calculate_pod_age(pod);
    let status = get_pod_state(pod);
    let selectors = pod.metadata.labels.clone();

    let resource_events = list_events_for_resource(events.to_vec(), &instance_name).await?;

    let (cpu_request, cpu_limit, memory_request, memory_limit) =
        pod.spec.as_ref().map_or((None, None, None, None), |spec| {
            let mut total_cpu_req = 0.0;
            let mut total_cpu_lim = 0.0;
            let mut total_mem_req = 0u64;
            let mut total_mem_lim = 0u64;
            let mut has_cpu_req = false;
            let mut has_cpu_lim = false;
            let mut has_mem_req = false;
            let mut has_mem_lim = false;

            for container in &spec.containers {
                if let Some(ref resources) = container.resources {
                    if let Some(ref requests) = resources.requests {
                        if let Some(cpu) = requests.get("cpu")
                            && let Some(val) = parse_cpu(&cpu.0)
                        {
                            total_cpu_req += val;
                            has_cpu_req = true;
                        }
                        if let Some(mem) = requests.get("memory")
                            && let Some(val) = parse_memory(&mem.0)
                        {
                            total_mem_req += val;
                            has_mem_req = true;
                        }
                    }
                    if let Some(ref limits) = resources.limits {
                        if let Some(cpu) = limits.get("cpu")
                            && let Some(val) = parse_cpu(&cpu.0)
                        {
                            total_cpu_lim += val;
                            has_cpu_lim = true;
                        }
                        if let Some(mem) = limits.get("memory")
                            && let Some(val) = parse_memory(&mem.0)
                        {
                            total_mem_lim += val;
                            has_mem_lim = true;
                        }
                    }
                }
            }

            (
                if has_cpu_req {
                    Some(format_cpu(total_cpu_req))
                } else {
                    None
                },
                if has_cpu_lim {
                    Some(format_cpu(total_cpu_lim))
                } else {
                    None
                },
                if has_mem_req {
                    Some(format_memory(total_mem_req))
                } else {
                    None
                },
                if has_mem_lim {
                    Some(format_memory(total_mem_lim))
                } else {
                    None
                },
            )
        });

    let (cpu_usage, memory_usage) =
        metrics_lookup
            .get(&instance_name)
            .map_or((None, None), |container_metrics| {
                let mut total_cpu_usage = 0.0;
                let mut total_mem_usage = 0u64;
                let mut has_cpu_usage = false;
                let mut has_mem_usage = false;

                for metric in container_metrics.values() {
                    if let Some(cpu) = metric.cpu_usage {
                        total_cpu_usage += cpu;
                        has_cpu_usage = true;
                    }
                    if let Some(mem) = metric.memory_usage {
                        total_mem_usage += mem;
                        has_mem_usage = true;
                    }
                }

                crate::cache_manager::record_pod_metrics(
                    &instance_name,
                    if has_cpu_usage {
                        Some(total_cpu_usage)
                    } else {
                        None
                    },
                    if has_mem_usage {
                        Some(total_mem_usage)
                    } else {
                        None
                    },
                );

                (
                    if has_cpu_usage {
                        Some(format_cpu(total_cpu_usage))
                    } else {
                        None
                    },
                    if has_mem_usage {
                        Some(format_memory(total_mem_usage))
                    } else {
                        None
                    },
                )
            });

    let node_name = pod.spec.as_ref().and_then(|spec| spec.node_name.clone());
    let (node_cpu_percent, node_memory_percent) = node_name
        .as_ref()
        .and_then(|name| node_info.get(name))
        .map_or((None, None), |(cpu, mem)| (Some(*cpu), Some(*mem)));

    Ok(RsPod {
        name: instance_name,
        status,
        description: kind.to_string(),
        age,
        containers: format!("{actual_container_count}/{desired_container_count}"),
        selectors,
        events: resource_events,
        cpu_request,
        cpu_limit,
        cpu_usage,
        memory_request,
        memory_limit,
        memory_usage,
        node_name,
        node_cpu_percent,
        node_memory_percent,
    })
}

/// Bundle of per-call auxiliary data consumed by `project_pod`.
///
/// Named so callers don't have to carry the 4-tuple shape around.
struct PodProjectionInputs {
    pods: Vec<Pod>,
    events: Vec<Event>,
    metrics_lookup: HashMap<String, HashMap<String, ContainerMetric>>,
    node_info: HashMap<String, (f64, f64)>,
}

/// Fetch pods, events, metrics, and nodes in parallel for projection.
///
/// Shared between `list_rspods` (label-selected, owner-filtered) and
/// `list_unowned_pods` (unselected, filter-predicate).
async fn load_pod_projection_inputs(
    client: &kube::Client,
    label_selector: &str,
) -> Result<PodProjectionInputs> {
    let namespace = get_current_namespace_or_default();
    let pods_api: Api<Pod> = Api::namespaced(client.clone(), &namespace);
    let nodes_api: Api<Node> = Api::all(client.clone());
    let lp = ListParams::default().labels(label_selector);
    let node_lp = ListParams::default();

    let (pod_list_result, events_result, pod_metrics_result, node_list_result, node_metrics_result) = tokio::join!(
        pods_api.list(&lp),
        list_k8sevents(client.clone()),
        fetch_pod_metrics(client.clone(), None),
        nodes_api.list(&node_lp),
        fetch_node_metrics(client.clone())
    );

    let pod_list = pod_list_result?;
    let events = events_result?;
    let pod_metrics = pod_metrics_result.unwrap_or_else(|e| {
        debug!("Could not fetch pod metrics: {}", e);
        Vec::new()
    });
    let metrics_lookup = create_metrics_lookup(pod_metrics);

    let node_list = node_list_result?;
    let node_metrics = node_metrics_result.unwrap_or_else(|e| {
        debug!("Could not fetch node metrics: {}", e);
        Vec::new()
    });
    let node_info = build_node_info(&node_list.items, &node_metrics);

    Ok(PodProjectionInputs {
        pods: pod_list.items,
        events,
        metrics_lookup,
        node_info,
    })
}

/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
pub async fn list_rspods(selector: BTreeMap<String, String>) -> Result<Vec<RsPod>> {
    let client = new(Some(USER_AGENT)).await?;
    let label_selector = format_label_selector(&selector);

    let inputs = load_pod_projection_inputs(&client, &label_selector).await?;

    let mut pod_vec = Vec::new();
    for pod in inputs.pods {
        if let Some(owners) = &pod.metadata.owner_references {
            for owner in owners {
                let rs_pod = project_pod(
                    &pod,
                    &owner.kind,
                    &inputs.events,
                    &inputs.metrics_lookup,
                    &inputs.node_info,
                )
                .await?;
                pod_vec.push(rs_pod);
            }
        }
    }

    Ok(pod_vec)
}

/// List pods no workload controller owns.
///
/// Covers pods with no `owner_references` and static pods (owner kind
/// `Node`) — e.g. kube-apiserver/scheduler/controller-manager on a kubeadm
/// cluster. Powers the synthesized "Unowned" row in the workloads landing.
///
/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
pub async fn list_unowned_pods() -> Result<Vec<RsPod>> {
    let client = new(Some(USER_AGENT)).await?;

    let inputs = load_pod_projection_inputs(&client, "").await?;

    let mut pod_vec = Vec::new();
    for pod in inputs.pods {
        if let Some(kind) = classify_unowned_pod(&pod) {
            let rs_pod = project_pod(
                &pod,
                kind,
                &inputs.events,
                &inputs.metrics_lookup,
                &inputs.node_info,
            )
            .await?;
            pod_vec.push(rs_pod);
        }
    }

    Ok(pod_vec)
}

/// List pods owned by a `Job` with the given name.
///
/// Jobs don't use label selection for pod lookup — the `owner_references`
/// chain is authoritative. Matches pods whose `owner_references` contain
/// any reference with `kind == "Job"` and `name == job_name`.
///
/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
pub async fn list_pods_by_job(job_name: String) -> Result<Vec<RsPod>> {
    let client = new(Some(USER_AGENT)).await?;

    let inputs = load_pod_projection_inputs(&client, "").await?;

    let mut pod_vec = Vec::new();
    for pod in inputs.pods {
        if pod_belongs_to_job(&pod, &job_name) {
            let rs_pod = project_pod(
                &pod,
                "Job",
                &inputs.events,
                &inputs.metrics_lookup,
                &inputs.node_info,
            )
            .await?;
            pod_vec.push(rs_pod);
        }
    }

    Ok(pod_vec)
}

/// Returns true if any `owner_references` entry on `pod` refers to a Job
/// with the given name.
fn pod_belongs_to_job(pod: &Pod, job_name: &str) -> bool {
    pod.metadata
        .owner_references
        .as_ref()
        .is_some_and(|refs| refs.iter().any(|o| o.kind == "Job" && o.name == job_name))
}

/// Classify a pod for the Unowned leaf.
///
/// Returns `Some("Unowned")` for pods with no `owner_references` (or an
/// empty list), `Some("StaticPod")` when every owner is a `Node` (kubelet
/// mirror pods like kube-apiserver/etcd on a kubeadm control plane), and
/// `None` when any workload controller owns the pod — those belong under
/// their workload row, not the Unowned leaf.
fn classify_unowned_pod(pod: &Pod) -> Option<&'static str> {
    match pod.metadata.owner_references.as_ref() {
        None => Some("Unowned"),
        Some(refs) if refs.is_empty() => Some("Unowned"),
        Some(refs) if refs.iter().all(|o| o.kind == "Node") => Some("StaticPod"),
        Some(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_unowned_pod, pod_belongs_to_job};
    use k8s_openapi::api::core::v1::Pod;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};

    fn pod_with_owners(owners: Option<Vec<OwnerReference>>) -> Pod {
        Pod {
            metadata: ObjectMeta {
                owner_references: owners,
                ..ObjectMeta::default()
            },
            ..Pod::default()
        }
    }

    fn owner(kind: &str) -> OwnerReference {
        OwnerReference {
            kind: kind.to_string(),
            ..OwnerReference::default()
        }
    }

    fn owner_named(kind: &str, name: &str) -> OwnerReference {
        OwnerReference {
            kind: kind.to_string(),
            name: name.to_string(),
            ..OwnerReference::default()
        }
    }

    #[test]
    fn classify_none_owners_is_unowned() {
        assert_eq!(
            classify_unowned_pod(&pod_with_owners(None)),
            Some("Unowned")
        );
    }

    #[test]
    fn classify_empty_owners_is_unowned() {
        assert_eq!(
            classify_unowned_pod(&pod_with_owners(Some(Vec::new()))),
            Some("Unowned"),
        );
    }

    #[test]
    fn classify_all_node_owners_is_static_pod() {
        assert_eq!(
            classify_unowned_pod(&pod_with_owners(Some(vec![owner("Node")]))),
            Some("StaticPod"),
        );
    }

    #[test]
    fn classify_workload_owner_returns_none() {
        assert_eq!(
            classify_unowned_pod(&pod_with_owners(Some(vec![owner("ReplicaSet")]))),
            None,
        );
    }

    #[test]
    fn classify_mixed_node_and_workload_owners_returns_none() {
        assert_eq!(
            classify_unowned_pod(&pod_with_owners(Some(vec![
                owner("Node"),
                owner("ReplicaSet")
            ]))),
            None,
        );
    }

    #[test]
    fn pod_belongs_to_job_matches_exact_job_name() {
        let pod = pod_with_owners(Some(vec![owner_named("Job", "backup-nightly")]));
        assert!(pod_belongs_to_job(&pod, "backup-nightly"));
    }

    #[test]
    fn pod_belongs_to_job_rejects_different_name() {
        let pod = pod_with_owners(Some(vec![owner_named("Job", "backup-nightly")]));
        assert!(!pod_belongs_to_job(&pod, "backup-weekly"));
    }

    #[test]
    fn pod_belongs_to_job_rejects_same_name_different_kind() {
        let pod = pod_with_owners(Some(vec![owner_named("ReplicaSet", "backup-nightly")]));
        assert!(!pod_belongs_to_job(&pod, "backup-nightly"));
    }

    #[test]
    fn pod_belongs_to_job_rejects_no_owners() {
        let pod = pod_with_owners(None);
        assert!(!pod_belongs_to_job(&pod, "backup-nightly"));
    }
}
