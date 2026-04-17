use crate::cache_manager::get_current_namespace_or_default;
use crate::error::Result;
use crate::k8s::events::{format_duration, list_events_for_resource, list_k8sevents};
use crate::tui::data::Rs;
use k8s_openapi::api::batch::v1::{CronJob, Job};
use kube::Api;
use kube::api::ListParams;
use kube::api::ObjectList;

use chrono::{DateTime, Utc};

use crate::k8s::client_manager::{get_client, refresh_client, should_refresh_client};

fn calculate_cronjob_age(cj: &CronJob) -> String {
    cj.metadata.creation_timestamp.as_ref().map_or_else(
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

/// List `CronJobs` in the current namespace, projected to the shared `Rs`
/// row shape so they can be merged into the workloads landing.
///
/// Unlike Jobs, `CronJobs` are shown regardless of whether they are
/// currently running — the landing should surface the schedule definitions
/// that exist, not just the ones that happen to be mid-tick. The `pods`
/// column shows the count of currently-active child Jobs
/// (`status.active.len()`), which is the operationally interesting number.
///
/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
#[allow(clippy::significant_drop_tightening)]
pub async fn list_cronjobs() -> Result<Vec<Rs>> {
    let mut client = get_client().await?;
    let namespace = get_current_namespace_or_default();

    let cj_list: ObjectList<CronJob> = {
        let api: Api<CronJob> = Api::namespaced((*client).clone(), &namespace);
        match api.list(&ListParams::default()).await {
            Ok(result) => result,
            Err(e) if should_refresh_client(&e) => {
                client = refresh_client().await?;
                let api: Api<CronJob> = Api::namespaced((*client).clone(), &namespace);
                api.list(&ListParams::default()).await?
            }
            Err(e) => return Err(e.into()),
        }
    };

    let mut cj_vec = Vec::new();

    let events = list_k8sevents((*client).clone()).await?;

    for cj in cj_list.items {
        let age = calculate_cronjob_age(&cj);
        let instance_name = cj.metadata.name.as_deref().unwrap_or("unknown").to_string();
        let f_instance_name = format!("{instance_name} ");

        let active_count = cj
            .status
            .as_ref()
            .and_then(|status| status.active.as_ref())
            .map_or(0, Vec::len);

        let resource_events = list_events_for_resource(events.clone(), &f_instance_name).await?;

        cj_vec.push(Rs {
            name: instance_name,
            pods: format!("{active_count}"),
            age,
            description: "CronJob".to_string(),
            owner: String::new(),
            selectors: None,
            events: resource_events,
        });
    }

    Ok(cj_vec)
}

/// Find the name of the most recent actively-running Job owned by the
/// given `CronJob`, if any.
///
/// Walks the `owner_references` chain `CronJob` → Job. Among Jobs whose
/// owners include a `CronJob` with `cj_name`, filters to those with
/// `status.active > 0`, then picks the one with the most recent
/// `creation_timestamp`. Returns `Ok(None)` when the `CronJob` has no
/// active child Job — the caller should stay on the landing rather than
/// routing to an empty pod list.
///
/// # Errors
///
/// Will return `Err` if the Jobs API call fails.
#[allow(clippy::significant_drop_tightening)]
pub async fn find_latest_active_job_for_cronjob(cj_name: &str) -> Result<Option<String>> {
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

    let latest = job_list
        .items
        .into_iter()
        .filter(|job| job_is_child_of_cronjob(job, cj_name))
        .filter(|job| job.status.as_ref().and_then(|s| s.active).unwrap_or(0) > 0)
        .max_by_key(|job| job.metadata.creation_timestamp.clone());

    Ok(latest.and_then(|j| j.metadata.name))
}

fn job_is_child_of_cronjob(job: &Job, cj_name: &str) -> bool {
    job.metadata.owner_references.as_ref().is_some_and(|refs| {
        refs.iter()
            .any(|o| o.kind == "CronJob" && o.name == cj_name)
    })
}

#[cfg(test)]
mod tests {
    use super::job_is_child_of_cronjob;
    use k8s_openapi::api::batch::v1::Job;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};

    fn job_with_owners(owners: Option<Vec<OwnerReference>>) -> Job {
        Job {
            metadata: ObjectMeta {
                owner_references: owners,
                ..ObjectMeta::default()
            },
            ..Job::default()
        }
    }

    fn owner(kind: &str, name: &str) -> OwnerReference {
        OwnerReference {
            kind: kind.to_string(),
            name: name.to_string(),
            ..OwnerReference::default()
        }
    }

    #[test]
    fn job_is_child_of_cronjob_matches_exact_name_and_kind() {
        let job = job_with_owners(Some(vec![owner("CronJob", "backup-nightly")]));
        assert!(job_is_child_of_cronjob(&job, "backup-nightly"));
    }

    #[test]
    fn job_is_child_of_cronjob_rejects_different_name() {
        let job = job_with_owners(Some(vec![owner("CronJob", "backup-nightly")]));
        assert!(!job_is_child_of_cronjob(&job, "backup-weekly"));
    }

    #[test]
    fn job_is_child_of_cronjob_rejects_same_name_different_kind() {
        let job = job_with_owners(Some(vec![owner("ReplicaSet", "backup-nightly")]));
        assert!(!job_is_child_of_cronjob(&job, "backup-nightly"));
    }

    #[test]
    fn job_is_child_of_cronjob_rejects_no_owners() {
        let job = job_with_owners(None);
        assert!(!job_is_child_of_cronjob(&job, "backup-nightly"));
    }
}
