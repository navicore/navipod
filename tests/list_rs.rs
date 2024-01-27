use navipod::k8s::rs::list_replicas;

#[tokio::test]
async fn test_list_replicas() {
    let data_result = list_replicas("default").await;
    assert!(matches!(data_result, Ok(..),));
    let data = &data_result.unwrap();
    assert_eq!(data.len(), 1);
    let data = &data[0];

    assert_eq!(data.owner, "my-navitain", "wrong deployment");
    assert_eq!(data.pods, "1/1", "wrong pod count");
    assert_eq!(data.description, "Deployment", "wrong rs kind");
}
