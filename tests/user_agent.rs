mod crypto_fixture;
use hyper::http::HeaderValue;

#[test]
fn test_valid_user_agent_header() {
    let user_agent_string = "navipod/1.0";
    // Test that valid user agent strings can be converted to HeaderValue
    assert!(HeaderValue::from_str(user_agent_string).is_ok());
}

#[test]
fn test_invalid_user_agent_header() {
    let invalid_user_agent = "\n\rInvalidAgent";
    // Test that invalid user agent strings fail HeaderValue validation
    assert!(HeaderValue::from_str(invalid_user_agent).is_err());
}

#[test]
fn test_user_agent_constant() {
    // Verify the USER_AGENT constant is properly formatted
    let ua = navipod::k8s::USER_AGENT;
    assert!(ua.contains("navipod/"));
    assert!(HeaderValue::from_str(ua).is_ok());
}

#[tokio::test]
async fn test_user_agent_client_new_with_default() {
    crypto_fixture::fixture();
    // Client creation should work with None (uses default kube-rs user-agent)
    let r = navipod::k8s::client::new(None).await;
    assert!(r.is_ok());
}

#[tokio::test]
async fn test_user_agent_client_new_with_valid_string() {
    crypto_fixture::fixture();
    let user_agent = "navipod/1.0";
    // Client creation should work with valid user agent
    let r = navipod::k8s::client::new(Some(user_agent)).await;
    assert!(r.is_ok());
}

#[tokio::test]
async fn test_user_agent_client_new_with_invalid_string() {
    crypto_fixture::fixture();
    let invalid_user_agent = "\n\rInvalidAgent";
    // Client creation should still succeed even with invalid user agent
    // (it will fall back to default kube-rs user-agent)
    let r = navipod::k8s::client::new(Some(invalid_user_agent)).await;
    assert!(r.is_ok());
}
