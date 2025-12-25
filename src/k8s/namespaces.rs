use crate::cache_manager::get_current_namespace_or_default;
use crate::error::Result;
use crate::k8s::events::format_duration;
use crate::tui::data::Namespace;
use k8s_openapi::api::core::v1::Namespace as K8sNamespace;
use kube::Api;
use kube::api::ListParams;
use tracing::{debug, error};

use chrono::{DateTime, Utc};

use crate::k8s::client_manager::{get_client, refresh_client, should_refresh_client};

fn calculate_namespace_age(ns: &K8sNamespace) -> String {
    ns.metadata.creation_timestamp.as_ref().map_or_else(
        || "Unk".to_string(),
        |creation_timestamp| {
            let ts: DateTime<_> = creation_timestamp.0;
            let now = Utc::now();
            let duration = now.signed_duration_since(ts);
            format_duration(duration)
        },
    )
}

/// List all namespaces in the cluster
///
/// # Errors
///
/// Will return `Err` if data cannot be retrieved from k8s cluster API
#[allow(clippy::cognitive_complexity)]
pub async fn list_namespaces() -> Result<Vec<Namespace>> {
    debug!("list_namespaces: starting namespace fetch");
    let mut client = get_client().await.map_err(|e| {
        error!("list_namespaces: failed to get client: {}", e);
        e
    })?;

    // Get current namespace to mark it in the list
    let current_namespace = get_current_namespace_or_default();
    debug!(
        "list_namespaces: current namespace is '{}'",
        current_namespace
    );

    // Namespaces are cluster-scoped, so we use Api::all
    let ns_list = {
        let api: Api<K8sNamespace> = Api::all((*client).clone());
        match api.list(&ListParams::default()).await {
            Ok(result) => {
                debug!("list_namespaces: fetched {} namespaces", result.items.len());
                result
            }
            Err(e) if should_refresh_client(&e) => {
                debug!("list_namespaces: refreshing client due to auth error");
                // Auth error - try refreshing client and retry once
                client = refresh_client().await?;
                let api: Api<K8sNamespace> = Api::all((*client).clone());
                api.list(&ListParams::default()).await?
            }
            Err(e) => {
                error!("list_namespaces: failed to list namespaces: {}", e);
                return Err(e.into());
            }
        }
    };

    let mut namespaces = Vec::new();

    for ns in ns_list.items {
        let name = ns
            .metadata
            .name
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let age = calculate_namespace_age(&ns);
        let status = ns
            .status
            .as_ref()
            .and_then(|s| s.phase.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        let is_current = name == current_namespace;

        namespaces.push(Namespace {
            name,
            status,
            age,
            is_current,
        });
    }

    // Sort alphabetically, but put current namespace first
    namespaces.sort_by(|a, b| match (a.is_current, b.is_current) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });

    Ok(namespaces)
}
