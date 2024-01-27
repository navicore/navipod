use k8s_openapi::api::apps::v1::ReplicaSet;
use kube::api::ListParams;
use kube::{Api, Client};

#[tokio::test]
async fn test_list_rs() {
    tracing_subscriber::fmt::init();
    let client_result = Client::try_default().await;
    assert!(matches!(client_result, Ok(..),));

    let client = client_result.unwrap();

    let namespace = "default";

    let replica_sets: Api<ReplicaSet> = Api::namespaced(client.clone(), namespace);
    let rs_list_result = replica_sets.list(&ListParams::default()).await;
    assert!(matches!(rs_list_result, Ok(..),));
    let rs_list = rs_list_result.unwrap();

    assert_eq!(rs_list.items.len(), 1);
    for rs in rs_list.iter() {
        if let Some(owners) = rs.metadata.owner_references.clone() {
            for owner in owners {
                let instance_name = &rs.metadata.name;
                let desired_replicas = &rs
                    .spec
                    .as_ref()
                    .map_or(0, |spec| spec.replicas.unwrap_or(0) as i32);
                let actual_replicas = &rs
                    .status
                    .as_ref()
                    .map_or(0, |status| status.replicas as i32);
                let kind = owner.kind;
                let name = owner.name;
                println!("{instance_name:?} belongs to {kind} {name} - pods: {desired_replicas}/{actual_replicas}");
                assert_eq!(kind, "Deployment", "not a deployment");
                assert_ne!(kind, "StatefulSet", "not a deployment");
            }
        }
    }
}
