use crate::k8s::scan::metrics;
use crate::k8s::{client, USER_AGENT};
use k8s_openapi::api::core::v1::Pod;
use kube::api::ObjectList;
use kube::{
    api::{Api, ListParams},
};
use sqlx::sqlite::SqlitePool;
use tracing::error;

/// # Errors
///
/// Will return `Err` if function cannot connect to Kubernetes
pub async fn fetch(
    namespace: String,
) -> Result<(ObjectList<Pod>, Api<Pod>), Box<dyn std::error::Error>> {
    let client = client::new(Some(USER_AGENT))
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    let lp = ListParams::default();
    let pods: Api<Pod> = Api::namespaced(client.clone(), namespace.as_str());

    let pod_list: ObjectList<Pod> = pods
        .list(&lp)
        .await
        .map_err(std::io::Error::other)?;

    Ok((pod_list, pods))
}

pub async fn gather_metrics(
    pool: &SqlitePool,
    pod_list: ObjectList<Pod>,
    pods: &Api<Pod>,
    namespace: String,
) {
    for p in pod_list.items {
        let metadata = p.metadata.clone();
        let metadata_name = metadata.name.unwrap_or_default();
        let labels = metadata.labels.unwrap_or_default();
        let appname = labels
            .get("app")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let annotations = metadata.annotations.unwrap_or_default();
        let scrape = annotations
            .get("prometheus.io/scrape")
            .cloned()
            .unwrap_or_else(|| "false".to_string());
        let path = annotations
            .get("prometheus.io/path")
            .cloned()
            .unwrap_or_else(|| "/metrics".to_string());
        let port = annotations
            .get("prometheus.io/port")
            .cloned()
            .unwrap_or_default();

        if scrape == "true" {
            let p = metrics::process(
                pool,
                pods,
                metadata_name.as_str(),
                path.as_str(),
                port.as_str(),
                appname.as_str(),
                namespace.as_str(),
            )
            .await;

            match p {
                Ok(()) => (),
                Err(e) => error!("Error processing metrics for {}: {:?}", metadata_name, e),
            }
        }
    }
}
