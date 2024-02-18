use navipod::k8s::rs::list_replicas;

#[tokio::test]
async fn test_list_replicas() {
    let data_result = list_replicas().await;
    assert!(matches!(data_result, Ok(..),));
    let data = &data_result.unwrap();
    assert_eq!(data.len(), 1);
    let data = &data[0];

    assert_eq!(data.owner, "my-navitain", "wrong deployment");
    assert_eq!(data.pods, "1/1", "wrong pod count");
    assert_eq!(data.description, "Deployment", "wrong rs kind");
}

#[tokio::test]
async fn test_list_replica_events() {
    let data_result = list_replicas().await;
    assert!(matches!(data_result, Ok(..),));
    let data = &data_result.unwrap();
    assert_eq!(data.len(), 1);
    let data = &data[0];
    let events = &data.events;
    //assert!(events.len() > 0);

    // println!("involved object {:?}", events[0].involved_object);
    // println!("reason {:?}", events[0].reason);
    // println!("reporting_component {:?}", events[0].reporting_component);
    // println!("reporting_instance {:?}", events[0].reporting_instance);
    // println!("message {:?}", events[0].message);
    // println!("series {:?}", events[0].series);
    // println!("metadata {:?}", events[0].metadata);
    //
    // println!("involved object {:?}", events[1].involved_object);
    // println!("reason {:?}", events[1].reason);
    // println!("reporting_component {:?}", events[1].reporting_component);
    // println!("reporting_instance {:?}", events[1].reporting_instance);
    // println!("message {:?}", events[1].message);
    // println!("series {:?}", events[1].series);
    // println!("metadata {:?}", events[1].metadata);
}
