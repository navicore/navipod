use crate::k8s::utils::format_label_selector;
use crate::tui::data::{Container, ContainerEnvVar, ContainerMount};
use k8s_openapi::api::core::v1::ContainerPort;
use k8s_openapi::api::core::v1::Pod;
use kube::api::ListParams;
use kube::api::ObjectList;
use kube::{Api, Client};
use std::collections::BTreeMap;

fn format_ports(ports: Option<Vec<ContainerPort>>) -> String {
    ports.map_or_else(
        || "no ports declaired".to_string(),
        |ports| {
            ports
                .iter()
                .map(|p| {
                    let port_name = p.name.as_deref().unwrap_or("unnamed"); // Use "unnamed" or any default string if name is None
                    format!("{}:{}", port_name, p.container_port)
                })
                .collect::<Vec<_>>()
                .join(", ")
        },
    )
}

/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
#[allow(clippy::significant_drop_tightening)]
pub async fn list(
    selector: BTreeMap<String, String>,
    pod_name: String,
) -> Result<Vec<Container>, kube::Error> {
    let client = Client::try_default().await?;

    let label_selector = format_label_selector(&selector);

    let lp = ListParams::default().labels(&label_selector);

    // Assuming there should be a single pod matching the selector and name
    let pod_list: ObjectList<Pod> = Api::default_namespaced(client).list(&lp).await?;

    let mut container_vec = Vec::new();

    for pod in pod_list.items {
        let container_statuses = pod
            .status
            .as_ref()
            .and_then(|status| status.container_statuses.clone())
            .unwrap_or_default();

        if let Some(name) = pod.metadata.name {
            if name == pod_name {
                if let Some(spec) = pod.spec {
                    for container in spec.containers {
                        let image = container.image.unwrap_or_else(|| "unknown".to_string());
                        let ports = format_ports(container.ports);
                        let restarts = container_statuses
                            .iter()
                            .find(|cs| cs.name == container.name)
                            .map_or(0, |cs| cs.restart_count)
                            .to_string();

                        let volume_mounts = container.volume_mounts;
                        let mounts: Vec<ContainerMount> = volume_mounts
                            .unwrap_or_else(Vec::new)
                            .into_iter()
                            .map(|vm| ContainerMount {
                                name: vm.name,
                                value: vm.mount_path,
                            })
                            .collect();

                        let env = container.env;
                        let envvars: Vec<ContainerEnvVar> = env
                            .unwrap_or_else(Vec::new)
                            .into_iter()
                            .map(|e| ContainerEnvVar {
                                name: e.name,
                                value: e.value.unwrap_or_default(),
                            })
                            .collect();

                        let c = Container {
                            name: container.name,
                            description: "a pod container".to_string(),
                            restarts,
                            image,
                            ports,
                            mounts,
                            envvars,
                        };
                        container_vec.push(c);
                    }

                    if let Some(init_containers) = spec.init_containers {
                        for container in init_containers {
                            let image = container.image.unwrap_or_else(|| "unknown".to_string());
                            let restarts = container_statuses
                                .iter()
                                .find(|cs| cs.name == container.name)
                                .map_or(0, |cs| cs.restart_count)
                                .to_string();

                            let volume_mounts = container.volume_mounts;
                            let mounts: Vec<ContainerMount> = volume_mounts
                                .unwrap_or_else(Vec::new)
                                .into_iter()
                                .map(|vm| ContainerMount {
                                    name: vm.name,
                                    value: vm.mount_path,
                                })
                                .collect();

                            let env = container.env;
                            let envvars: Vec<ContainerEnvVar> = env
                                .unwrap_or_else(Vec::new)
                                .into_iter()
                                .map(|e| ContainerEnvVar {
                                    name: e.name,
                                    value: e.value.unwrap_or_default(),
                                })
                                .collect();

                            let c = Container {
                                name: container.name,
                                description: "an init container".to_string(), // Distinguish init containers
                                restarts,
                                image,
                                ports: String::new(),
                                mounts,
                                envvars,
                            };
                            container_vec.push(c);
                        }
                    }
                }
            }
        }
    }

    Ok(container_vec)
}
