mod crypto_fixture;
use navipod::k8s::client::UserAgentError;

#[tokio::test]
async fn test_user_agent_layer() {
    let user_agent_string = "navipod/1.0";
    assert!(UserAgentError::new(user_agent_string).is_ok());
}

// #[test]
// fn test_user_agent_layer_new_with_invalid_string() {
//     let invalid_user_agent = "\u{007F}InvalidAgent";
//     assert!(UserAgentError::new(invalid_user_agent).is_err());
// }

// #[tokio::test]
// async fn test_user_agent_client_new_with_default() {
//     crypto_fixture::fixture();
//     let r = navipod::k8s::client::new(None).await;
//     assert!(r.is_ok());
// }
//
// #[tokio::test]
// async fn test_user_agent_client_new_with_valid_string() {
//     crypto_fixture::fixture();
//     let user_agent = "navipod/1.0";
//     let r = navipod::k8s::client::new(Some(user_agent)).await;
//     assert!(r.is_ok());
// }

// #[tokio::test]
// async fn test_user_agent_client_new_with_invalid_string() {
//     crypto_fixture::fixture();
//     let invalid_user_agent = "\u{007F}InvalidAgent";
//     let r = navipod::k8s::client::new(Some(invalid_user_agent)).await;
//     assert!(r.is_err());
// }
