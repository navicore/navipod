use itertools::Itertools;
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
pub fn generate_container_recs() -> Vec<Container> {
    use fakeit::generator;

    (0..2)
        .map(|_| {
            let container = generator::generate("???????????".to_string());
            let description = "Pod Container".to_string();

            Container {
                name: container,
                description,
            }
        })
        .sorted_by(|a, b| a.name.cmp(&b.name))
        .collect_vec()
}

#[derive(Clone, Debug)]
pub struct Pod {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) age: String,
    pub(crate) containers: String,
}

impl Pod {
    pub(crate) const fn ref_array(&self) -> [&String; 4] {
        [
            &self.name,
            &self.description,
            &self.age,
            &self.containers,
        ]
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
pub fn generate_pod_recs() -> Vec<Pod> {
    use fakeit::generator;

    (0..20)
        .map(|_| {
            let podname = generator::generate("replica###-??#?#?##-??#?#?#".to_string());
            let description = "Deployment Pod".to_string();
            let age = "200d".to_string();
            let containers = "2/2".to_string();

            Pod {
                name: podname,
                description,
                age,
                containers,
            }
        })
        .sorted_by(|a, b| a.name.cmp(&b.name))
        .collect_vec()
}
#[derive(Clone, Debug)]
pub struct Rs {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) age: String,
    pub(crate) pods: String,
    pub(crate) containers: String,
}

impl Rs {
    pub(crate) const fn ref_array(&self) -> [&String; 5] {
        [
            &self.name,
            &self.description,
            &self.age,
            &self.pods,
            &self.containers,
        ]
    }

    pub(crate) fn replicaset(&self) -> &str {
        &self.name
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
pub fn generate_rs_recs() -> Vec<Rs> {
    use fakeit::generator;

    (0..20)
        .map(|_| {
            let replicaset = generator::generate("replica###-??#?#?##".to_string());
            let description = "Deployment".to_string();
            let age = "200d".to_string();
            let pods = "4/4".to_string();
            let containers = "8/8".to_string();

            Rs {
                name: replicaset,
                description,
                age,
                pods,
                containers,
            }
        })
        .sorted_by(|a, b| a.name.cmp(&b.name))
        .collect_vec()
}

#[allow(clippy::cast_possible_truncation)]
pub fn rs_constraint_len_calculator(items: &[Rs]) -> (u16, u16, u16, u16, u16) {
    let name_len = items
        .iter()
        .map(Rs::replicaset)
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
        description_len as u16,
        age_len as u16,
        pods_len as u16,
        containers_len as u16,
    )
}
#[allow(clippy::cast_possible_truncation)]
pub fn pod_constraint_len_calculator(items: &[Pod]) -> (u16, u16, u16, u16) {
    let name_len = items
        .iter()
        .map(Pod::podname)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let description_len = items
        .iter()
        .map(Pod::description)
        .flat_map(str::lines)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let age_len = items
        .iter()
        .map(Pod::age)
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let containers_len = items
        .iter()
        .map(Pod::containers)
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
    use crate::tui::data::{Container, container_constraint_len_calculator, Pod, pod_constraint_len_calculator, Rs, rs_constraint_len_calculator};

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
            Pod {
                name: "replica-123456-123456".to_string(),
                description: "Deployment".to_string(),
                age: "150d".to_string(),
                containers: "2/2".to_string(),
            },
            Pod {
                name: "replica-923450-987654".to_string(),
                description: "Deployment".to_string(),
                age: "10d".to_string(),
                containers: "2/2".to_string(),
            },
        ];
        let (longest_pod_name_len, longest_description_len, longest_age_len, longest_containers_len) =
            pod_constraint_len_calculator(&test_data);

        assert_eq!(21, longest_pod_name_len);
        assert_eq!(10, longest_description_len);
        assert_eq!(4, longest_age_len);
        assert_eq!(3, longest_containers_len);
    }
    #[test]
    fn test_rs_constraint_len_calculator() {
        let test_data = vec![
            Rs {
                name: "replica-123456".to_string(),
                description: "Deployment".to_string(),
                age: "300d".to_string(),
                pods: "10/10".to_string(),
                containers: "19/30".to_string(),
            },
            Rs {
                name: "replica-923450".to_string(),
                description: "Deployment".to_string(),
                age: "10d".to_string(),
                pods: "1/1".to_string(),
                containers: "2/2".to_string(),
            },
        ];
        let (
            longest_replicaset_len,
            longest_description_len,
            longest_age_len,
            longest_pods_len,
            longest_containers_len,
        ) = rs_constraint_len_calculator(&test_data);

        assert_eq!(14, longest_replicaset_len);
        assert_eq!(10, longest_description_len);
        assert_eq!(4, longest_age_len);
        assert_eq!(5, longest_pods_len);
        assert_eq!(5, longest_containers_len);
    }
}
