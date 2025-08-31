use crate::error::Result;
use crate::k8s::events::{format_duration, list_events_for_resource, list_k8sevents};
use crate::k8s::utils::format_label_selector;
use crate::tui::data::RsPod;
use chrono::{DateTime, Utc};
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::api::ListParams;
use kube::api::ObjectList;
use std::collections::BTreeMap;

use super::client::new;

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
    }

    "Unknown".to_string()
}

/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
#[allow(clippy::significant_drop_tightening)]
pub async fn list_rspods(selector: BTreeMap<String, String>) -> Result<Vec<RsPod>> {
    let client = new(None).await?;

    // Format the label selector from the BTreeMap
    let label_selector = format_label_selector(&selector);

    // Apply the label selector in ListParams
    let lp = ListParams::default().labels(&label_selector);

    let pod_list: ObjectList<Pod> = Api::default_namespaced(client.clone()).list(&lp).await?;

    let mut pod_vec = Vec::new();

    // get all events from the cluster to avoid calls for each pod
    let events = list_k8sevents(client).await?;

    for pod in pod_list.items {
        if let Some(owners) = &pod.metadata.owner_references {
            for owner in owners {
                let instance_name = &pod
                    .metadata
                    .name
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()); // Fixed typo in "unknown"

                // Adjusted actual container count to reflect only ready containers
                let actual_container_count = pod.status.as_ref().map_or(0, |status| {
                    status
                        .container_statuses
                        .as_ref()
                        .map_or(0, |container_statuses| {
                            container_statuses.iter().filter(|cs| cs.ready).count()
                        })
                });

                // Desired container count remains the same
                let desired_container_count =
                    pod.spec.as_ref().map_or(0, |spec| spec.containers.len());
                let kind = &owner.kind;

                let age = calculate_pod_age(&pod);
                let status = get_pod_state(&pod);
                let selectors = pod.metadata.labels.clone();

                let resource_events =
                    list_events_for_resource(events.clone(), instance_name).await?;

                let data = RsPod {
                    name: instance_name.to_string(),
                    status: status.to_string(),
                    description: kind.to_string(),
                    age,
                    containers: format!("{actual_container_count}/{desired_container_count}"),
                    selectors,
                    events: resource_events,
                };

                pod_vec.push(data);
            }
        }
    }

    Ok(pod_vec)
}
