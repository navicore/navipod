use k8s_openapi::api::apps::v1::ReplicaSet;
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::api::core::v1::Service;
use k8s_openapi::api::networking::v1::Ingress;
use kube::api::ListParams;
use kube::{Api, Client};

use super::client::k8s_client;

/// # Errors
///
/// Will return `Err` if function cannot connect to Kubernetes
pub async fn explain(namespace: &str, pod_name: &str) -> Result<(), kube::Error> {
    let client = k8s_client().await?;
    let pod = get_pod(&client, namespace, pod_name).await?;

    check_replica_set(&client, &pod, namespace).await?;
    check_services(&client, &pod, namespace).await?;
    check_ingresses(&client, &pod, namespace).await?;

    drop(client);
    Ok(())
}

async fn get_pod(client: &Client, namespace: &str, pod_name: &str) -> Result<Pod, kube::Error> {
    let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);
    pods.get(pod_name).await
}

async fn check_replica_set(client: &Client, pod: &Pod, namespace: &str) -> Result<(), kube::Error> {
    let replica_sets: Api<ReplicaSet> = Api::namespaced(client.clone(), namespace);
    let rs_list = replica_sets.list(&ListParams::default()).await?;
    drop(replica_sets);

    for rs in rs_list.iter() {
        if let Some(rs_ref) = rs.spec.as_ref() {
            if let Some(selector) = rs_ref.selector.match_labels.clone() {
                if matches_pod_labels(pod, &selector) {
                    if let Some(owners) = rs.metadata.owner_references.clone() {
                        for owner in owners {
                            let kind = owner.kind;
                            let name = owner.name;
                            println!("Belongs to {kind} {name}");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn services_for_pod(
    client: &Client,
    pod: &Pod,
    namespace: &str,
) -> Result<Vec<String>, kube::Error> {
    let services: Api<Service> = Api::namespaced(client.clone(), namespace);
    let service_list = services.list(&ListParams::default()).await?;
    drop(services);

    Ok(service_list
        .iter()
        .filter_map(|service| {
            if let Some(svc_ref) = service.spec.as_ref() {
                if let Some(selector) = svc_ref.selector.clone() {
                    if matches_pod_labels(pod, &selector) {
                        return service.metadata.name.clone();
                    }
                }
            }
            None
        })
        .collect())
}

async fn check_services(client: &Client, pod: &Pod, namespace: &str) -> Result<(), kube::Error> {
    let services_names = services_for_pod(client, pod, namespace).await?;
    for svc_name in &services_names {
        println!("Implements Service {svc_name}");
    }
    Ok(())
}

fn matches_pod_labels(pod: &Pod, selector: &std::collections::BTreeMap<String, String>) -> bool {
    selector.iter().all(|(key, value)| {
        pod.metadata
            .labels
            .as_ref()
            .map_or(false, |labels| labels.get(key.as_str()) == Some(value))
    })
}

async fn check_ingresses(client: &Client, pod: &Pod, namespace: &str) -> Result<(), kube::Error> {
    let ingresses: Api<Ingress> = Api::namespaced(client.clone(), namespace);
    let services = services_for_pod(client, pod, namespace).await?;

    let ingress_list = ingresses.list(&ListParams::default()).await?;
    drop(ingresses);
    for ingress in ingress_list.iter() {
        if let Some(rules_ref) = &ingress.spec.as_ref() {
            handle_ingress_rules(&rules_ref.rules, &services, ingress);
        }
    }

    Ok(())
}

fn handle_ingress_rules(
    rules: &Option<Vec<k8s_openapi::api::networking::v1::IngressRule>>,
    services: &[String],
    ingress: &Ingress,
) {
    if let Some(rules) = rules {
        for rule in rules {
            handle_ingress_rule(rule, services, ingress);
        }
    }
}

fn handle_ingress_rule(
    rule: &k8s_openapi::api::networking::v1::IngressRule,
    services: &[String],
    ingress: &Ingress,
) {
    if let Some(http) = &rule.http {
        for path in &http.paths {
            handle_http_path(path, services, ingress, rule.host.as_deref());
        }
    }
}

fn handle_http_path(
    path: &k8s_openapi::api::networking::v1::HTTPIngressPath,
    services: &[String],
    ingress: &Ingress,
    host: Option<&str>,
) {
    if let Some(backend_service_name) = &path.backend.service {
        if services.contains(&backend_service_name.name) {
            print_ingress_info(ingress, host, path, backend_service_name);
        }
    }
}

fn print_ingress_info(
    ingress: &Ingress,
    host: Option<&str>,
    path: &k8s_openapi::api::networking::v1::HTTPIngressPath,
    backend_service_name: &k8s_openapi::api::networking::v1::IngressServiceBackend,
) {
    let ingress_name = ingress.metadata.name.as_deref().unwrap_or("");
    let path_txt = path.path.clone().unwrap_or_default();
    if let Some(port_info) = backend_service_name.port.clone() {
        let port_num = port_info.number.unwrap_or(0);
        if port_num <= 0 {
            println!(
                "Ingress {} routes {}{} to pod via Service {}",
                ingress_name,
                host.unwrap_or(""),
                path_txt,
                backend_service_name.name
            );
        } else {
            println!(
                "Ingress {} routes {}{} to pod via Service {} on port {}",
                ingress_name,
                host.unwrap_or(""),
                path_txt,
                backend_service_name.name,
                port_num
            );
        }
    }
}
