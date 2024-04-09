use navipod::k8s::pods::list_rspods;
use navipod::k8s::rs::list_replicas;

#[tokio::test]
async fn test_list_pods() {
    let _ =
        rustls::crypto::CryptoProvider::install_default(rustls::crypto::ring::default_provider());
    let data_result = list_replicas().await;
    assert!(matches!(data_result, Ok(..),));
    let data = &data_result.unwrap();
    assert_eq!(data.len(), 2);

    let data = &data[0];
    let selectors_opt = &data.selectors;
    assert!(selectors_opt.is_some());
    let selector = selectors_opt.clone().unwrap();

    let data_result = list_rspods(selector).await;
    assert!(matches!(data_result, Ok(..),));
    let data = &data_result.unwrap();
    let data = &data[0];

    assert_eq!(data.containers, "2/2", "wrong pod count"); // assumes will always be echo-secret sorted first
    assert_eq!(data.description, "ReplicaSet", "wrong rs kind");
}
