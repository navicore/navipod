use crate::k8s::events::{format_duration, list_events_for_resource};
use k8s_openapi::api::core::v1::Pod;
use kube::api::ListParams;
use kube::api::ObjectList;
use kube::{Api, Client};
use std::collections::BTreeMap;

use crate::tui::data::{Container, RsPod};

use chrono::{DateTime, Utc};

fn calculate_pod_age(pod: &Pod) -> String {
    pod.metadata.creation_timestamp.as_ref().map_or_else(
        || "Unk".to_string(),
        |creation_timestamp| {
            let ts: DateTime<_> = creation_timestamp.0;
            let now = Utc::now();
            let duration = now.signed_duration_since(ts);
            format_duration(duration)
        },
    )
}

fn format_label_selector(selector: &BTreeMap<String, String>) -> String {
    selector
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<String>>()
        .join(",")
}

fn convert_to_containers(names: Vec<String>) -> Vec<Container> {
    names
        .into_iter()
        .map(|name| Container {
            name,
            description: "Pod Container".to_string(),
        })
        .collect()
}

fn get_container_names(pod: &Pod) -> Vec<String> {
    // Check if the pod spec exists and if it contains containers
    pod.spec.as_ref().map_or_else(Vec::new, |spec| {
        spec.containers
            .iter()
            .map(|container| container.name.clone())
            .collect()
    })
}

fn get_pod_state(pod: &Pod) -> String {
    // Check if the pod is marked for deletion
    if pod.metadata.deletion_timestamp.is_some() {
        return "Terminating".to_string();
    }

    // Then proceed to check the pod's status as before
    if let Some(status) = &pod.status {
        if let Some(phase) = &status.phase {
            return match phase.as_str() {
                "Pending" => "Pending".to_string(),
                "Running" => {
                    if status.conditions.as_ref().map_or(false, |conds| {
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
    }

    "Unknown".to_string()
}

/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
pub async fn list_rspods(selector: BTreeMap<String, String>) -> Result<Vec<RsPod>, kube::Error> {
    let client = Client::try_default().await?;

    // Format the label selector from the BTreeMap
    let label_selector = format_label_selector(&selector);

    // Apply the label selector in ListParams
    let lp = ListParams::default().labels(&label_selector);

    let pod_list: ObjectList<Pod> = Api::default_namespaced(client.clone()).list(&lp).await?;

    let mut pod_vec = Vec::new();

    for pod in pod_list.items {
        let container_names = get_container_names(&pod);
        if let Some(owners) = &pod.metadata.owner_references {
            for owner in owners {
                let instance_name = &pod
                    .metadata
                    .name
                    .clone()
                    .unwrap_or_else(|| "unkown".to_string());

                let actual_container_count = pod.status.as_ref().map_or(0, |status| {
                    status.container_statuses.as_ref().map_or(0, Vec::len)
                });

                // Desired container count
                let desired_container_count =
                    pod.spec.as_ref().map_or(0, |spec| spec.containers.len());
                let kind = &owner.kind;

                let age = calculate_pod_age(&pod);
                let status = get_pod_state(&pod);
                let selectors = pod.metadata.labels.as_ref().map(std::clone::Clone::clone);
                let data = RsPod {
                    name: instance_name.to_string(),
                    status: status.to_string(),
                    description: kind.to_string(),
                    age,
                    containers: format!("{actual_container_count}/{desired_container_count}"),
                    container_names: convert_to_containers(container_names.clone()),
                    selectors,
                    events: list_events_for_resource(client.clone(), instance_name).await?,
                };

                pod_vec.push(data);
            }
        }
    }

    Ok(pod_vec)
}
