use navipod::k8s::pods::list_rspods;
use navipod::k8s::rs::list_replicas;

#[tokio::test]
async fn test_list_pods() {
    let data_result = list_replicas().await;
    assert!(matches!(data_result, Ok(..),));
    let data = &data_result.unwrap();
    assert_eq!(data.len(), 1);
    let data = &data[0];
    let selectors_opt = &data.selectors;
    assert!(selectors_opt.is_some());
    let selector = selectors_opt.clone().unwrap();

    let data_result = list_rspods(selector).await;
    assert!(matches!(data_result, Ok(..),));
    let data = &data_result.unwrap();
    let data = &data[0];

    assert_eq!(data.containers, "1/1", "wrong pod count");
    assert_eq!(data.description, "ReplicaSet", "wrong rs kind");
}
