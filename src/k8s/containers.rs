use crate::error::Result;
use crate::k8s::client_manager::{get_client, refresh_client, should_refresh_client};
use crate::k8s::utils::format_label_selector;
use crate::tui::data::{Container, ContainerEnvVar, ContainerMount, LogRec};
use k8s_openapi::api::core::v1::ContainerPort;
use k8s_openapi::api::core::v1::Pod;
use kube::{
    ResourceExt,
    api::{Api, ListParams, LogParams, ObjectList},
};
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
#[allow(clippy::too_many_lines)]
pub async fn list(selector: BTreeMap<String, String>, pod_name: String) -> Result<Vec<Container>> {
    let mut client = get_client().await?;
    let label_selector = format_label_selector(&selector);
    let lp = ListParams::default().labels(&label_selector);

    // Try the operation, with one retry on auth error
    let pod_list: ObjectList<Pod> = match Api::default_namespaced((*client).clone()).list(&lp).await {
        Ok(result) => result,
        Err(e) if should_refresh_client(&e) => {
            // Auth error - try refreshing client and retry once
            client = refresh_client().await?;
            Api::default_namespaced((*client).clone()).list(&lp).await?
        }
        Err(e) => return Err(e.into()),
    };

    let mut container_vec = Vec::new();

    for pod in pod_list.items {
        let container_statuses = pod
            .status
            .as_ref()
            .and_then(|status| status.container_statuses.clone())
            .unwrap_or_default();

        if let Some(name) = pod.metadata.name {
            if name == pod_name.clone() {
            let container_selectors = pod.metadata.labels;
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
                            selectors: container_selectors.clone(),
                            pod_name: pod_name.clone(),
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
                                selectors: container_selectors.clone(),
                                pod_name: pod_name.clone(),
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

/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
#[allow(clippy::significant_drop_tightening)]
pub async fn logs(
    selector: BTreeMap<String, String>,
    pod_name: String,
    container_name: String,
) -> Result<Vec<LogRec>> {
    let mut client = get_client().await?;
    let label_selector = format_label_selector(&selector);
    let lp = ListParams::default().labels(&label_selector);

    // Try the operation, with one retry on auth error
    let pod_list: ObjectList<Pod> = {
        let pods = Api::default_namespaced((*client).clone());
        match pods.list(&lp).await {
            Ok(result) => result,
            Err(e) if should_refresh_client(&e) => {
                // Auth error - try refreshing client and retry once
                client = refresh_client().await?;
                let pods = Api::default_namespaced((*client).clone());
                pods.list(&lp).await?
            }
            Err(e) => return Err(e.into()),
        }
    };

    let mut log_vec = Vec::new();

    // Find the pod by name
    for pod in pod_list
        .items
        .into_iter()
        .filter(|pod| pod.name_any() == pod_name)
    {
        let log_params = LogParams {
            container: Some(container_name.clone()),
            tail_lines: Some(100), // Adjust based on how many lines you want
            ..Default::default()
        };

        // Fetch logs for the specified container, with retry on auth error
        let pods: Api<Pod> = Api::default_namespaced((*client).clone());
        let logs = match pods.logs(&pod.name_any(), &log_params).await {
            Ok(result) => result,
            Err(e) if should_refresh_client(&e) => {
                // Auth error - try refreshing client and retry once
                client = refresh_client().await?;
                let pods: Api<Pod> = Api::default_namespaced((*client).clone());
                pods.logs(&pod.name_any(), &log_params).await?
            }
            Err(e) => return Err(e.into()),
        };

        // Parse and map logs to Vec<Log>
        logs.lines().for_each(|line| {
            log_vec.push(LogRec {
                datetime: String::new(), //need a smart parser that can figure out the format
                level: String::new(),
                message: line.to_string(),
            });
        });
    }
    log_vec.reverse(); // Reverse the order of logs to show the latest logs first

    Ok(log_vec)
}
