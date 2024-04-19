use kube::Client;

/// Create a new k8s client to interact with k8s cluster api
///
/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
pub async fn k8s_client() -> Result<Client, kube::Error> {
    // create a new k8s client that has a custom agent identifier
    let client = Client::try_default().await?;

    // how do I modify the client to have a custom agent identifier?

    Ok(client)
}
