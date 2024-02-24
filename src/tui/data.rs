use std::collections::BTreeMap;
use unicode_width::UnicodeWidthStr;

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct ResourceEvent {
    pub resource_name: String,
    pub message: String,
    pub reason: String,
    pub type_: String,
    pub age: String,
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct ContainerMount {
    pub name: String,
    pub value: String,
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct ContainerEnvVar {
    pub name: String,
    pub value: String,
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct Cert {
    pub host: String,
    pub is_valid: String,
    pub expires: String,
    pub issued_by: String,
}

impl Cert {
    pub(crate) const fn ref_array(&self) -> [&String; 4] {
        [&self.host, &self.is_valid, &self.expires, &self.issued_by]
    }

    pub(crate) fn host(&self) -> &str {
        &self.host
    }

    pub(crate) fn is_valid(&self) -> &str {
        &self.is_valid
    }

    pub(crate) fn expires(&self) -> &str {
        &self.expires
    }

    pub(crate) fn issued_by(&self) -> &str {
        &self.issued_by
    }
}

#[derive(Clone, Debug)]
pub struct Container {
    pub name: String,
    pub description: String,
    pub restarts: String,
    pub image: String,
    pub ports: String,
    pub envvars: Vec<ContainerEnvVar>,
    pub mounts: Vec<ContainerMount>,
}

impl Container {
    pub(crate) const fn ref_array(&self) -> [&String; 5] {
        [
            &self.name,
            &self.description,
            &self.restarts,
            &self.image,
            &self.ports,
        ]
    }

    pub(crate) fn container(&self) -> &str {
        &self.name
    }

    pub(crate) fn description(&self) -> &str {
        &self.description
    }

    pub(crate) fn restarts(&self) -> &str {
        &self.restarts
    }

    pub(crate) fn image(&self) -> &str {
        &self.image
    }

    pub(crate) fn ports(&self) -> &str {
        &self.ports
    }
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct RsPod {
    pub name: String,
    pub status: String,
    pub description: String,
    pub age: String,
    pub containers: String,
    pub selectors: Option<BTreeMap<String, String>>,
    pub events: Vec<ResourceEvent>,
}

impl RsPod {
    pub(crate) const fn ref_array(&self) -> [&String; 5] {
        [
            &self.name,
            &self.status,
            &self.containers,
            &self.age,
            &self.description,
        ]
    }

    pub(crate) fn status(&self) -> &str {
        &self.status
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn description(&self) -> &str {
        &self.description
    }

    pub(crate) fn age(&self) -> &str {
        &self.age
    }

    pub(crate) fn containers(&self) -> &str {
        &self.containers
    }
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct Rs {
    pub name: String,
    pub owner: String,
    pub description: String,
    pub age: String,
    pub pods: String,
    pub selectors: Option<BTreeMap<String, String>>,
    pub events: Vec<ResourceEvent>,
}

impl Rs {
    pub(crate) const fn ref_array(&self) -> [&String; 5] {
        [
            &self.name,
            &self.pods,
            &self.age,
            &self.description,
            &self.owner,
        ]
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn owner(&self) -> &str {
        &self.owner
    }

    pub(crate) fn description(&self) -> &str {
        &self.description
    }

    pub(crate) fn age(&self) -> &str {
        &self.age
    }

    pub(crate) fn pods(&self) -> &str {
        &self.pods
    }
}

#[derive(Clone, Debug)]
pub struct Ingress {
    pub name: String,
    pub host: String,
    pub path: String,
    pub backend_svc: String,
    pub port: String,
}

impl Ingress {
    pub(crate) const fn ref_array(&self) -> [&String; 5] {
        [
            &self.name,
            &self.host,
            &self.path,
            &self.backend_svc,
            &self.port,
        ]
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn host(&self) -> &str {
        &self.host
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn backend_svc(&self) -> &str {
        &self.backend_svc
    }

    pub(crate) fn port(&self) -> &str {
        &self.port
    }
}

#[allow(clippy::cast_possible_truncation)]
pub fn ingress_constraint_len_calculator(items: &[Ingress]) -> (u16, u16, u16, u16, u16) {
    let name_len = items
        .iter()
        .map(Ingress::name)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let host_len = items
        .iter()
        .map(Ingress::host)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let path_len = items
        .iter()
        .map(Ingress::path)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let backend_svc_len = items
        .iter()
        .map(Ingress::backend_svc)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let port_len = items
        .iter()
        .map(Ingress::port)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    (
        name_len as u16,
        host_len as u16,
        path_len as u16,
        backend_svc_len as u16,
        port_len as u16,
    )
}

#[allow(clippy::cast_possible_truncation)]
pub fn rs_constraint_len_calculator(items: &[Rs]) -> (u16, u16, u16, u16, u16) {
    let name_len = items
        .iter()
        .map(Rs::name)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let pods_len = items
        .iter()
        .map(Rs::pods)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let age_len = items
        .iter()
        .map(Rs::age)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let description_len = items
        .iter()
        .map(Rs::description)
        .flat_map(str::lines)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let owner_len = items
        .iter()
        .map(Rs::owner)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);

    (
        name_len as u16,
        pods_len as u16,
        age_len as u16,
        description_len as u16,
        owner_len as u16,
    )
}

#[allow(clippy::cast_possible_truncation)]
pub fn pod_constraint_len_calculator(items: &[RsPod]) -> (u16, u16, u16, u16, u16) {
    let name_len = items
        .iter()
        .map(RsPod::name)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let status_len = items
        .iter()
        .map(RsPod::status)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let description_len = items
        .iter()
        .map(RsPod::description)
        .flat_map(str::lines)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let age_len = items
        .iter()
        .map(RsPod::age)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let containers_len = items
        .iter()
        .map(RsPod::containers)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);

    (
        name_len as u16,
        status_len as u16,
        containers_len as u16,
        age_len as u16,
        description_len as u16,
    )
}

#[allow(clippy::cast_possible_truncation)]
pub fn cert_constraint_len_calculator(items: &[Cert]) -> (u16, u16, u16, u16) {
    let host_len = items
        .iter()
        .map(Cert::host)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let valid_len = items
        .iter()
        .map(Cert::is_valid)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let expires_len = items
        .iter()
        .map(Cert::expires)
        .flat_map(str::lines)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let issued_by_len = items
        .iter()
        .map(Cert::issued_by)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    (
        host_len as u16,
        valid_len as u16,
        expires_len as u16,
        issued_by_len as u16,
    )
}

#[allow(clippy::cast_possible_truncation)]
pub fn container_constraint_len_calculator(items: &[Container]) -> (u16, u16, u16, u16, u16) {
    let name_len = items
        .iter()
        .map(Container::container)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let description_len = items
        .iter()
        .map(Container::description)
        .flat_map(str::lines)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let restarts_len = items
        .iter()
        .map(Container::restarts)
        .flat_map(str::lines)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let image_len = items
        .iter()
        .map(Container::image)
        .flat_map(str::lines)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let ports_len = items
        .iter()
        .map(Container::ports)
        .flat_map(str::lines)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);

    (
        name_len as u16,
        description_len as u16,
        restarts_len as u16,
        image_len as u16,
        ports_len as u16,
    )
}

#[cfg(test)]
mod tests {
    use crate::tui::data::{
        container_constraint_len_calculator, pod_constraint_len_calculator,
        rs_constraint_len_calculator, Container, Rs, RsPod,
    };

    #[test]
    fn test_container_constraint_len_calculator() {
        let test_data = vec![
            Container {
                name: "replica-123456-123456".to_string(),
                description: "Deployment".to_string(),
                restarts: "0".to_string(),
                image: "navicore/echo-secret-py:v0.1.1".to_string(),
                ports: "http:1234".to_string(),
                envvars: vec![],
                mounts: vec![],
            },
            Container {
                name: "replica-923450-987654".to_string(),
                description: "Deployment".to_string(),
                restarts: "0".to_string(),
                image: "navicore/echo-secret-py:v0.1.1".to_string(),
                ports: "http:1234".to_string(),
                envvars: vec![],
                mounts: vec![],
            },
        ];
        let (
            longest_container_len,
            longest_description_len,
            longest_restarts_len,
            longest_image_len,
            longest_ports_len,
        ) = container_constraint_len_calculator(&test_data);

        assert_eq!(21, longest_container_len);
        assert_eq!(10, longest_description_len);
        assert_eq!(1, longest_restarts_len);
        assert_eq!(30, longest_image_len);
        assert_eq!(9, longest_ports_len);
    }
    #[test]
    fn test_pod_constraint_len_calculator() {
        let test_data = vec![
            RsPod {
                name: "replica-123456-123456".to_string(),
                status: "Running".to_string(),
                description: "Deployment".to_string(),
                age: "150d".to_string(),
                containers: "2/2".to_string(),
                selectors: None,
                events: vec![],
            },
            RsPod {
                name: "replica-923450-987654".to_string(),
                status: "Terminating".to_string(),
                description: "Deployment".to_string(),
                age: "10d".to_string(),
                containers: "2/2".to_string(),
                selectors: None,
                events: vec![],
            },
        ];
        let (
            longest_pod_name_len,
            longest_status_len,
            longest_containers_len,
            longest_age_len,
            longest_description_len,
        ) = pod_constraint_len_calculator(&test_data);

        assert_eq!(21, longest_pod_name_len);
        assert_eq!(11, longest_status_len);
        assert_eq!(10, longest_description_len);
        assert_eq!(4, longest_age_len);
        assert_eq!(3, longest_containers_len);
    }
    #[test]
    fn test_rs_constraint_len_calculator() {
        let test_data = vec![
            Rs {
                name: "my-replica-123456".to_string(),
                owner: "my-replica".to_string(),
                description: "Deployment".to_string(),
                age: "300d".to_string(),
                pods: "10/10".to_string(),
                selectors: None,
                events: vec![],
            },
            Rs {
                name: "my-replica-923450".to_string(),
                owner: "my-replica".to_string(),
                description: "Deployment".to_string(),
                age: "10d".to_string(),
                pods: "1/1".to_string(),
                selectors: None,
                events: vec![],
            },
        ];
        let (
            longest_name_len,
            longest_pods_len,
            longest_age_len,
            longest_description_len,
            longest_owner_len,
        ) = rs_constraint_len_calculator(&test_data);

        assert_eq!(17, longest_name_len);
        assert_eq!(10, longest_owner_len);
        assert_eq!(10, longest_description_len);
        assert_eq!(4, longest_age_len);
        assert_eq!(5, longest_pods_len);
    }
}
