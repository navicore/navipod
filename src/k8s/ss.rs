use crate::cache_manager::get_current_namespace_or_default;
use crate::error::Result;
use crate::k8s::events::{format_duration, list_events_for_resource, list_k8sevents};
use crate::tui::data::Rs;
use k8s_openapi::api::apps::v1::StatefulSet;
use kube::Api;
use kube::api::ListParams;
use kube::api::ObjectList;

use chrono::{DateTime, Utc};

use crate::k8s::client_manager::{get_client, refresh_client, should_refresh_client};

fn calculate_ss_age(ss: &StatefulSet) -> String {
    ss.metadata.creation_timestamp.as_ref().map_or_else(
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

/// List `StatefulSets` in the current namespace, projected to the shared `Rs`
/// row shape so they can be merged into the workloads landing.
///
/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
#[allow(clippy::significant_drop_tightening)]
pub async fn list_statefulsets() -> Result<Vec<Rs>> {
    let mut client = get_client().await?;
    let namespace = get_current_namespace_or_default();

    let ss_list: ObjectList<StatefulSet> = {
        let api: Api<StatefulSet> = Api::namespaced((*client).clone(), &namespace);
        match api.list(&ListParams::default()).await {
            Ok(result) => result,
            Err(e) if should_refresh_client(&e) => {
                client = refresh_client().await?;
                let api: Api<StatefulSet> = Api::namespaced((*client).clone(), &namespace);
                api.list(&ListParams::default()).await?
            }
            Err(e) => return Err(e.into()),
        }
    };

    let mut ss_vec = Vec::new();

    let events = list_k8sevents((*client).clone()).await?;

    for ss in ss_list.items {
        // Prefer spec.selector.match_labels for pod lookup (authoritative for
        // which pods the StatefulSet owns). Fall back to metadata.labels only
        // if spec.selector is absent.
        let selectors = ss
            .spec
            .as_ref()
            .and_then(|spec| spec.selector.match_labels.clone())
            .or_else(|| ss.metadata.labels.clone());

        let age = calculate_ss_age(&ss);
        let instance_name = ss.metadata.name.as_deref().unwrap_or("unknown").to_string();
        let f_instance_name = format!("{instance_name} ");

        let desired = ss.spec.as_ref().and_then(|spec| spec.replicas).unwrap_or(0);
        let ready = ss
            .status
            .as_ref()
            .and_then(|status| status.ready_replicas)
            .unwrap_or(0);

        let resource_events = list_events_for_resource(events.clone(), &f_instance_name).await?;

        ss_vec.push(Rs {
            name: instance_name,
            pods: format!("{ready}/{desired}"),
            age,
            description: "StatefulSet".to_string(),
            owner: String::new(),
            selectors,
            events: resource_events,
        });
    }

    Ok(ss_vec)
}
