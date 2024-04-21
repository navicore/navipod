use crate::error::Result;
use crate::k8s::events::{format_duration, list_events_for_resource, list_k8sevents};
use crate::k8s::utils::format_label_selector;
use crate::tui::data::Rs;
use k8s_openapi::api::apps::v1::ReplicaSet;
use kube::api::ListParams;
use kube::api::ObjectList;
use kube::{Api, Client};
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use super::client::new;

fn calculate_rs_age(rs: &ReplicaSet) -> String {
    rs.metadata.creation_timestamp.as_ref().map_or_else(
        || "Unk".to_string(),
        |creation_timestamp| {
            let ts: DateTime<_> = creation_timestamp.0;
            let now = Utc::now();
            let duration = now.signed_duration_since(ts);
            format_duration(duration)
        },
    )
}

/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
#[allow(clippy::significant_drop_tightening)]
pub async fn list_replicas() -> Result<Vec<Rs>> {
    let client = new(None).await?;

    let rs_list: ObjectList<ReplicaSet> = Api::default_namespaced(client.clone())
        .list(&ListParams::default())
        .await?;

    let mut rs_vec = Vec::new();

    // get all events from the cluster to avoid calls for each rs
    let events = list_k8sevents(client).await?;

    for rs in rs_list.items {
        if let Some(owners) = &rs.metadata.owner_references {
            for owner in owners {
                let selectors = rs.metadata.labels.clone();

                let age = calculate_rs_age(&rs);
                let instance_name = &rs.metadata.name.as_deref().unwrap_or("unknown").to_string();
                let f_instance_name = format!("{instance_name} "); //padding for just high level
                let desired_replicas = &rs
                    .spec
                    .as_ref()
                    .map_or(0, |spec| spec.replicas.unwrap_or(0));
                let ready_replicas = &rs
                    .status
                    .as_ref()
                    .map_or(0, |status| status.ready_replicas.unwrap_or(0));
                let kind = &owner.kind;
                let owner_name = &owner.name;

                let resource_events =
                    list_events_for_resource(events.clone(), &f_instance_name).await?;
                let data = Rs {
                    name: instance_name.to_string(),
                    pods: format!("{ready_replicas}/{desired_replicas}"),
                    age,
                    description: kind.to_string(),
                    owner: owner_name.to_owned(),
                    selectors,
                    events: resource_events,
                };

                if desired_replicas <= &0 {
                    continue;
                };
                rs_vec.push(data);
            }
        }
    }

    Ok(rs_vec)
}

/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
pub async fn get_replicaset(selector: BTreeMap<String, String>) -> Result<Option<ReplicaSet>> {
    let client = Client::try_default().await?;

    let label_selector = format_label_selector(&selector);

    let lp = ListParams::default().labels(&label_selector);

    let rs_list: ObjectList<ReplicaSet> = Api::default_namespaced(client.clone()).list(&lp).await?;

    let rs = rs_list.into_iter().next();
    Ok(rs)
}
