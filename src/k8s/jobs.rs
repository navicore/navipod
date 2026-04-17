use crate::cache_manager::get_current_namespace_or_default;
use crate::error::Result;
use crate::k8s::events::{format_duration, list_events_for_resource, list_k8sevents};
use crate::tui::data::Rs;
use k8s_openapi::api::batch::v1::Job;
use kube::Api;
use kube::api::ListParams;
use kube::api::ObjectList;

use chrono::{DateTime, Utc};

use crate::k8s::client_manager::{get_client, refresh_client, should_refresh_client};

fn calculate_job_age(job: &Job) -> String {
    job.metadata.creation_timestamp.as_ref().map_or_else(
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

/// List currently-active Jobs in the current namespace, projected to the
/// shared `Rs` row shape so they can be merged into the workloads landing.
///
/// Only Jobs with `status.active > 0` are returned. Completed Jobs
/// (succeeded or failed with no active pods) are suppressed — they're
/// not operationally interesting once their pods are gone.
///
/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
#[allow(clippy::significant_drop_tightening)]
pub async fn list_jobs() -> Result<Vec<Rs>> {
    let mut client = get_client().await?;
    let namespace = get_current_namespace_or_default();

    let job_list: ObjectList<Job> = {
        let api: Api<Job> = Api::namespaced((*client).clone(), &namespace);
        match api.list(&ListParams::default()).await {
            Ok(result) => result,
            Err(e) if should_refresh_client(&e) => {
                client = refresh_client().await?;
                let api: Api<Job> = Api::namespaced((*client).clone(), &namespace);
                api.list(&ListParams::default()).await?
            }
            Err(e) => return Err(e.into()),
        }
    };

    let mut job_vec = Vec::new();

    let events = list_k8sevents((*client).clone()).await?;

    for job in job_list.items {
        let active = job
            .status
            .as_ref()
            .and_then(|status| status.active)
            .unwrap_or(0);
        if active <= 0 {
            continue;
        }

        let age = calculate_job_age(&job);
        let instance_name = job
            .metadata
            .name
            .as_deref()
            .unwrap_or("unknown")
            .to_string();
        let f_instance_name = format!("{instance_name} ");

        let succeeded = job
            .status
            .as_ref()
            .and_then(|status| status.succeeded)
            .unwrap_or(0);

        let resource_events = list_events_for_resource(events.clone(), &f_instance_name).await?;

        job_vec.push(Rs {
            name: instance_name,
            pods: format!("{active}/{succeeded}"),
            age,
            description: "Job".to_string(),
            owner: String::new(),
            selectors: None,
            events: resource_events,
        });
    }

    Ok(job_vec)
}
