use crate::tui::data::RsPod;
use k8s_openapi::api::core::v1::Pod;
use kube::api::ListParams;
use kube::api::ObjectList;
use kube::{Api, Client};
use std::collections::BTreeMap;

// return true if all of selector pairs can be found in pod labels
fn select<K, V>(selector: &BTreeMap<K, V>, pod_labels: &BTreeMap<K, V>) -> bool
where
    K: Ord + Eq,
    V: Eq,
{
    selector
        .iter()
        .all(|(key, value)| pod_labels.get(key) == Some(value))
}

/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
pub async fn list_rspods(selector: BTreeMap<String, String>) -> Result<Vec<RsPod>, kube::Error> {
    let client = Client::try_default().await?;

    let pod_list: ObjectList<Pod> = Api::default_namespaced(client.clone())
        .list(&ListParams::default())
        .await?;

    let mut pod_vec = Vec::new();

    for pod in pod_list.items {
        if let Some(owners) = pod.metadata.owner_references {
            for owner in owners {
                if let Some(ref map) = pod.metadata.labels {
                    if !select(&selector, map) {
                        continue;
                    };
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
                    let kind = owner.kind;

                    let data = RsPod {
                        name: instance_name.to_string(),
                        description: kind,
                        age: "???".to_string(),
                        containers: format!("{actual_container_count}/{desired_container_count}"),
                    };

                    pod_vec.push(data);
                }
            }
        }
    }

    Ok(pod_vec)
}
