use crate::tui::data::{ResourceEvent, Rs};
use k8s_openapi::api::apps::v1::ReplicaSet;
use k8s_openapi::api::core::v1::Event;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
use kube::api::ListParams;
use kube::api::ObjectList;
use kube::{Api, Client};
use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};

fn calculate_event_age(event_time: Option<&Time>) -> String {
    event_time.map_or_else(String::new, |time| {
        let now = Utc::now();
        let event_datetime: DateTime<Utc> = time.0;
        let duration = now.signed_duration_since(event_datetime);
        format_duration(duration)
    })
}

// Conversion function
fn convert_event_to_resource_event(event: &Event, rs_name: &str) -> ResourceEvent {
    let pattern = format!("{rs_name} ");
    let message = event
        .message
        .as_deref()
        .unwrap_or_default()
        .replace(&pattern, "");
    let reason = event.reason.clone().unwrap_or_default();
    let type_ = event.type_.clone().unwrap_or_default();
    let age = calculate_event_age(event.last_timestamp.as_ref());

    ResourceEvent {
        resource_name: rs_name.to_string(),
        message,
        reason,
        type_,
        age,
    }
}

async fn list_events_for_resource(
    client: Client,
    rs_name: &str,
) -> Result<Vec<ResourceEvent>, kube::Error> {
    let lp = ListParams::default();

    let mut filtered_events: Vec<Event> = Api::default_namespaced(client)
        .list(&lp)
        .await?
        .items
        .into_iter()
        .filter(|e: &Event| e.message.as_deref().unwrap_or_default().contains(rs_name))
        .collect();

    filtered_events.sort_by(|a, b| {
        b.last_timestamp
            .clone()
            .map_or_else(chrono::Utc::now, |t| t.0)
            .cmp(
                &a.last_timestamp
                    .clone()
                    .map_or_else(chrono::Utc::now, |t| t.0),
            )
    });

    let mut resource_events: Vec<ResourceEvent> = filtered_events
        .iter()
        .map(|e| convert_event_to_resource_event(e, rs_name))
        .collect();

    resource_events.retain(|e| !e.age.is_empty());

    Ok(resource_events)
}

fn format_duration(duration: Duration) -> String {
    if duration.num_days() > 0 {
        format!("{}d", duration.num_days())
    } else if duration.num_hours() > 0 {
        format!("{}h", duration.num_hours())
    } else if duration.num_minutes() > 0 {
        format!("{}m", duration.num_minutes())
    } else {
        format!("{}s", duration.num_seconds())
    }
}

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
pub async fn list_replicas() -> Result<Vec<Rs>, kube::Error> {
    let client = Client::try_default().await?;

    let rs_list: ObjectList<ReplicaSet> = Api::default_namespaced(client.clone())
        .list(&ListParams::default())
        .await?;

    let mut rs_vec = Vec::new();

    for rs in rs_list.items {
        if let Some(owners) = &rs.metadata.owner_references {
            for owner in owners {
                let selectors = rs.metadata.labels.as_ref().map(std::clone::Clone::clone);

                let age = calculate_rs_age(&rs);
                let instance_name = &rs
                    .metadata
                    .name
                    .clone()
                    .unwrap_or_else(|| "unkown".to_string());
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

                let data = Rs {
                    name: instance_name.to_string(),
                    pods: format!("{ready_replicas}/{desired_replicas}"),
                    age,
                    description: kind.to_string(),
                    owner: owner_name.to_owned(),
                    selectors,
                    events: list_events_for_resource(client.clone(), instance_name).await?,
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

fn format_label_selector(selector: &BTreeMap<String, String>) -> String {
    selector
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<String>>()
        .join(",")
}

/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
pub async fn get_replicaset(
    selector: BTreeMap<String, String>,
) -> Result<Option<ReplicaSet>, kube::Error> {
    let client = Client::try_default().await?;

    let label_selector = format_label_selector(&selector);

    let lp = ListParams::default().labels(&label_selector);

    let rs_list: ObjectList<ReplicaSet> = Api::default_namespaced(client.clone()).list(&lp).await?;

    let rs = rs_list.into_iter().next();
    Ok(rs)
}
