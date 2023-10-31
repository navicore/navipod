use k8s_openapi::api::apps::v1::ReplicaSet;
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::api::core::v1::Service;
use k8s_openapi::api::networking::v1::Ingress;
use kube::api::ListParams;
use kube::{Api, Client};
use tracing::debug;

/// # Errors
///
/// Will return `Err` if function cannot connect to Kubernetes
pub async fn explain(namespace: &str, pod_name: &str) -> Result<(), kube::Error> {
    let client = Client::try_default().await?;
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
    let ingress_list = ingresses.list(&ListParams::default()).await?;
    drop(ingresses);
    let services_for_pod = services_for_pod(client, pod, namespace).await?;

    for ingress in ingress_list.iter() {
        if let Some(rules_ref) = &ingress.spec.as_ref() {
            if let Some(rules) = rules_ref.rules.clone() {
                for rule in rules {
                    if let Some(http) = &rule.http {
                        if let Some(host) = &rule.host {
                            for path in &http.paths {
                                if let Some(backend_service_name) = &path.backend.service {
                                    debug!("checking ingress backend service: {backend_service_name:?}");
                                    if services_for_pod.contains(&backend_service_name.name) {
                                        if let Some(ingress_name) = ingress.metadata.name.as_ref() {
                                            if let Some(port_info) =
                                                backend_service_name.port.clone()
                                            {
                                                let port_num = port_info.number.unwrap_or(0);
                                                if let Some(path_txt) = path.path.clone() {
                                                    if port_num <= 0 {
                                                        println!("Ingress {ingress_name} routes {host}{path_txt} \
                                                            to pod via Service {}", backend_service_name.name);
                                                    } else {
                                                        println!("Ingress {ingress_name} routes {host}{path_txt} \
                                                            to pod via Service {} on port {}",
                                                            backend_service_name.name, port_num);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
