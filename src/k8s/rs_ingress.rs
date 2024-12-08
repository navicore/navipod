use crate::error::Result;
use crate::tui::data;
use k8s_openapi::api::apps::v1::ReplicaSet;
use k8s_openapi::api::core::v1::Service;
use k8s_openapi::api::networking::v1::Ingress;
use kube::api::ListParams;
use kube::{Api, Client};

use super::client::new;

fn matches_rs_labels(
    rs: &ReplicaSet,
    selector: &std::collections::BTreeMap<String, String>,
) -> bool {
    selector.iter().all(|(key, value)| {
        rs.metadata
            .labels
            .as_ref()
            .map_or(false, |labels| labels.get(key.as_str()) == Some(value))
    })
}

async fn services_for_rs(client: &Client, rs: &ReplicaSet, namespace: &str) -> Result<Vec<String>> {
    let services: Api<Service> = Api::namespaced(client.clone(), namespace);
    let service_list = services.list(&ListParams::default()).await?;
    drop(services);

    Ok(service_list
        .iter()
        .filter_map(|service| {
            if let Some(svc_ref) = service.spec.as_ref() {
                if let Some(selector) = svc_ref.selector.clone() {
                    if matches_rs_labels(rs, &selector) {
                        return service.metadata.name.clone();
                    }
                }
            }
            None
        })
        .collect())
}

/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
pub async fn list_ingresses(rs: &ReplicaSet, namespace: &str) -> Result<Vec<data::Ingress>> {
    let client = new(None).await?;

    let ingresses: Api<Ingress> = Api::namespaced(client.clone(), namespace);
    let services = services_for_rs(&client, rs, namespace).await?;
    drop(client);

    let ingress_list = ingresses.list(&ListParams::default()).await?;
    drop(ingresses);

    let mut all_ingresses = Vec::new();

    for ingress in ingress_list {
        if let Some(rules_ref) = ingress.spec.as_ref().map(|spec| &spec.rules) {
            let ingresses_for_rule = handle_ingress_rules(rules_ref.as_ref(), &services, &ingress);
            all_ingresses.extend(ingresses_for_rule);
        }
    }

    Ok(all_ingresses)
}

fn handle_ingress_rules(
    rules: Option<&Vec<k8s_openapi::api::networking::v1::IngressRule>>,
    services: &[String],
    ingress: &Ingress,
) -> Vec<data::Ingress> {
    // Note the return type Vec<data::Ingress>
    rules.as_ref().map_or_else(
        Vec::new, // If rules is None, return an empty Vec
        |rules| {
            rules
                .iter()
                .flat_map(|rule| handle_ingress_rule(rule, services, ingress)) // Flatten all Vec<data::Ingress> into one Vec
                .collect()
        },
    )
}

fn handle_ingress_rule(
    rule: &k8s_openapi::api::networking::v1::IngressRule,
    services: &[String],
    ingress: &Ingress,
) -> Vec<data::Ingress> {
    rule.http.as_ref().map_or_else(Vec::new, |http| {
        http.paths
            .iter()
            .filter_map(|path| handle_http_path(path, services, ingress, rule.host.as_deref()))
            .collect()
    })
}

fn handle_http_path(
    path: &k8s_openapi::api::networking::v1::HTTPIngressPath,
    services: &[String],
    ingress: &Ingress,
    host: Option<&str>,
) -> Option<data::Ingress> {
    path.backend
        .service
        .as_ref()
        .and_then(|backend_service_name| {
            if services.contains(&backend_service_name.name) {
                get_rs_ingress_info(ingress, host, path, backend_service_name)
            } else {
                None
            }
        })
}

pub(crate) fn get_rs_ingress_info(
    ingress: &Ingress,
    host: Option<&str>,
    path: &k8s_openapi::api::networking::v1::HTTPIngressPath,
    backend_service_name: &k8s_openapi::api::networking::v1::IngressServiceBackend,
) -> Option<data::Ingress> {
    let ingress_name = ingress.metadata.name.as_deref().unwrap_or("");
    let path_txt = path.path.clone().unwrap_or_default();
    if let Some(port_info) = backend_service_name.port.clone() {
        let port_num = port_info.number.unwrap_or(0);
        let port_txt = if port_num <= 0 {
            String::new()
        } else {
            port_num.to_string()
        };
        Some(data::Ingress {
            name: ingress_name.to_string(),
            host: host.unwrap_or("").to_string(),
            path: path_txt,
            backend_svc: backend_service_name.name.to_string(),
            port: port_txt,
        })
    } else {
        None
    }
}
