use crate::tui::data::Rs;
use k8s_openapi::api::apps::v1::ReplicaSet;
use kube::api::ListParams;
use kube::api::ObjectList;
use kube::{Api, Client};

/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
pub async fn list_replicas() -> Result<Vec<Rs>, kube::Error> {
    let client = Client::try_default().await?;

    let rs_list: ObjectList<ReplicaSet> = Api::default_namespaced(client.clone())
        .list(&ListParams::default())
        .await?;

    let mut rs_vec = Vec::new();

    for rs in rs_list.items {
        if let Some(owners) = rs.metadata.owner_references {
            for owner in owners {
                let instance_name = &rs
                    .metadata
                    .name
                    .clone()
                    .unwrap_or_else(|| "unkown".to_string());
                let desired_replicas = &rs
                    .spec
                    .as_ref()
                    .map_or(0, |spec| spec.replicas.unwrap_or(0));
                if desired_replicas <= &0 {
                    continue;
                };
                let actual_replicas = &rs.status.as_ref().map_or(0, |status| status.replicas);
                let kind = owner.kind;
                let owner_name = owner.name;

                let data = Rs {
                    name: instance_name.to_string(),
                    owner: owner_name,
                    description: kind,
                    age: "???".to_string(),
                    pods: format!("{actual_replicas}/{desired_replicas}"),
                    containers: "?/?".to_string(),
                };

                rs_vec.push(data);
            }
        }
    }

    Ok(rs_vec)
}
