use kube::Client;
use navipod::k8s::events::list_events_for_resource;
use navipod::k8s::rs::list_replicas;

#[tokio::test]
async fn test_list_replicas() {
    let data_result = list_replicas().await;
    assert!(matches!(data_result, Ok(..),));
    let data = &data_result.unwrap();
    assert_eq!(data.len(), 2);
    let data = &data[1]; // assumes echo-secret sorted first

    assert_eq!(data.owner, "my-navitain", "wrong deployment");
    assert_eq!(data.pods, "1/1", "wrong pod count");
    assert_eq!(data.description, "Deployment", "wrong rs kind");
}

#[tokio::test]
async fn test_list_replica_events() {
    let data_result = list_replicas().await;
    assert!(matches!(data_result, Ok(..),));
    let data = &data_result.unwrap();
    assert_eq!(data.len(), 2);
}

#[tokio::test]
async fn test_list_events_for_resource() {
    let client = Client::try_default().await.unwrap();

    let _ = list_events_for_resource(client, "my_stuff").await.unwrap();
}
