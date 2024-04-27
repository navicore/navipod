use crate::tui::data::ResourceEvent;
use k8s_openapi::api::core::v1::Event;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
use kube::api::ListParams;
use kube::{Api, Client};

use chrono::{DateTime, Duration, Utc};

fn calculate_event_age(event_time: Option<&Time>) -> String {
    event_time.map_or_else(String::new, |time| {
        let now = Utc::now();
        let event_datetime: DateTime<Utc> = time.0;
        let duration = now.signed_duration_since(event_datetime);
        format_duration(duration)
    })
}

fn convert_event_to_resource_event(event: &Event, rs_name: &str) -> ResourceEvent {
    let message = event
        .message
        .as_deref()
        .unwrap_or_default()
        .replace(rs_name, "")
        .replace("combined from similar events", "combined ")
        .trim()
        .trim_end_matches(':')
        .to_string();

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

/// # Errors
///
/// Will return `Err` if events cannot be retrieved from k8s cluster api
pub async fn list_k8sevents(client: Client) -> Result<Vec<Event>, kube::Error> {
    let lp = ListParams::default();

    let mut unfiltered_events: Vec<Event> = Api::default_namespaced(client).list(&lp).await?.items;

    unfiltered_events.sort_by(|a, b| {
        b.last_timestamp
            .clone()
            .map_or_else(chrono::Utc::now, |t| t.0)
            .cmp(
                &a.last_timestamp
                    .clone()
                    .map_or_else(chrono::Utc::now, |t| t.0),
            )
    });

    Ok(unfiltered_events)
}

/// # Errors
///
/// Will return `Err` if events cannot be retrieved from k8s cluster api
pub async fn _list_all(client: Client) -> Result<Vec<ResourceEvent>, kube::Error> {
    let lp = ListParams::default();

    let mut unfiltered_events: Vec<Event> = Api::default_namespaced(client).list(&lp).await?.items;

    unfiltered_events.sort_by(|a, b| {
        b.last_timestamp
            .clone()
            .map_or_else(chrono::Utc::now, |t| t.0)
            .cmp(
                &a.last_timestamp
                    .clone()
                    .map_or_else(chrono::Utc::now, |t| t.0),
            )
    });

    let mut resource_events: Vec<ResourceEvent> = unfiltered_events
        .iter()
        .map(|e| convert_event_to_resource_event(e, ""))
        .collect();

    resource_events.retain(|e| !e.age.is_empty());

    Ok(resource_events)
}

/// # Errors
///
/// Will return `Err` if data can not be extracted from events
pub async fn list_events_for_resource(
    events: Vec<Event>,
    resource_name: &str,
) -> Result<Vec<ResourceEvent>, kube::Error> {
    let mut filtered_events: Vec<Event> = events
        .into_iter()
        .filter(|e: &Event| {
            e.message
                .as_deref()
                .unwrap_or_default()
                .contains(resource_name)
                || e.metadata
                    .name
                    .clone()
                    .unwrap_or_default()
                    .contains(resource_name)
        })
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
        .map(|e| convert_event_to_resource_event(e, resource_name))
        .collect();

    resource_events.retain(|e| !e.age.is_empty());

    Ok(resource_events)
}

#[must_use]
pub fn format_duration(duration: Duration) -> String {
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
