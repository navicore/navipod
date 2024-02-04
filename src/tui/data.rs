use std::collections::BTreeMap;

use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug)]
pub struct Container {
    pub name: String,
    pub description: String,
}

impl Container {
    pub(crate) const fn ref_array(&self) -> [&String; 2] {
        [&self.name, &self.description]
    }

    pub(crate) fn container(&self) -> &str {
        &self.name
    }

    pub(crate) fn description(&self) -> &str {
        &self.description
    }
}

#[derive(Clone, Debug)]
pub struct RsPod {
    pub name: String,
    pub description: String,
    pub age: String,
    pub containers: String,
    pub container_names: Vec<Container>,
}

impl RsPod {
    pub(crate) const fn ref_array(&self) -> [&String; 4] {
        [&self.name, &self.description, &self.age, &self.containers]
    }

    pub(crate) fn podname(&self) -> &str {
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

#[derive(Clone, Debug)]
pub struct Rs {
    pub name: String,
    pub owner: String,
    pub description: String,
    pub age: String,
    pub pods: String,
    pub containers: String,
    pub selectors: Option<BTreeMap<String, String>>,
}

impl Rs {
    pub(crate) const fn ref_array(&self) -> [&String; 6] {
        [
            &self.name,
            &self.owner,
            &self.description,
            &self.age,
            &self.pods,
            &self.containers,
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

    pub(crate) fn containers(&self) -> &str {
        &self.containers
    }
}

#[allow(clippy::cast_possible_truncation)]
pub fn rs_constraint_len_calculator(items: &[Rs]) -> (u16, u16, u16, u16, u16, u16) {
    let name_len = items
        .iter()
        .map(Rs::name)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let owner_len = items
        .iter()
        .map(Rs::owner)
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
    let age_len = items
        .iter()
        .map(Rs::age)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let pods_len = items
        .iter()
        .map(Rs::pods)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let containers_len = items
        .iter()
        .map(Rs::containers)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);

    (
        name_len as u16,
        owner_len as u16,
        description_len as u16,
        age_len as u16,
        pods_len as u16,
        containers_len as u16,
    )
}

#[allow(clippy::cast_possible_truncation)]
pub fn pod_constraint_len_calculator(items: &[RsPod]) -> (u16, u16, u16, u16) {
    let name_len = items
        .iter()
        .map(RsPod::podname)
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
        description_len as u16,
        age_len as u16,
        containers_len as u16,
    )
}
#[allow(clippy::cast_possible_truncation)]
pub fn container_constraint_len_calculator(items: &[Container]) -> (u16, u16) {
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

    (name_len as u16, description_len as u16)
}
#[cfg(test)]
mod tests {
    use crate::tui::data::{
        Container, container_constraint_len_calculator,
        pod_constraint_len_calculator, Rs, rs_constraint_len_calculator, RsPod,
    };

    #[test]
    fn test_container_constraint_len_calculator() {
        let test_data = vec![
            Container {
                name: "replica-123456-123456".to_string(),
                description: "Deployment".to_string(),
            },
            Container {
                name: "replica-923450-987654".to_string(),
                description: "Deployment".to_string(),
            },
        ];
        let (longest_container_len, longest_description_len) =
            container_constraint_len_calculator(&test_data);

        assert_eq!(21, longest_container_len);
        assert_eq!(10, longest_description_len);
    }
    #[test]
    fn test_pod_constraint_len_calculator() {
        let test_data = vec![
            RsPod {
                name: "replica-123456-123456".to_string(),
                description: "Deployment".to_string(),
                age: "150d".to_string(),
                containers: "2/2".to_string(),
                container_names: vec![],
            },
            RsPod {
                name: "replica-923450-987654".to_string(),
                description: "Deployment".to_string(),
                age: "10d".to_string(),
                containers: "2/2".to_string(),
                container_names: vec![],
            },
        ];
        let (
            longest_pod_name_len,
            longest_description_len,
            longest_age_len,
            longest_containers_len,
        ) = pod_constraint_len_calculator(&test_data);

        assert_eq!(21, longest_pod_name_len);
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
                containers: "19/30".to_string(),
                selectors: None,
            },
            Rs {
                name: "my-replica-923450".to_string(),
                owner: "my-replica".to_string(),
                description: "Deployment".to_string(),
                age: "10d".to_string(),
                pods: "1/1".to_string(),
                containers: "2/2".to_string(),
                selectors: None,
            },
        ];
        let (
            longest_name_len,
            longest_owner_len,
            longest_description_len,
            longest_age_len,
            longest_pods_len,
            longest_containers_len,
        ) = rs_constraint_len_calculator(&test_data);

        assert_eq!(17, longest_name_len);
        assert_eq!(10, longest_owner_len);
        assert_eq!(10, longest_description_len);
        assert_eq!(4, longest_age_len);
        assert_eq!(5, longest_pods_len);
        assert_eq!(5, longest_containers_len);
    }
}
