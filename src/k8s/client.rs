use crate::error::Result;
use hyper_util::rt::TokioExecutor;
use kube::{client::ConfigExt, Client, Config};
/// Create a new k8s client to interact with k8s cluster api
///
/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
pub async fn k8s_client() -> Result<Client> {
    // modify the client to have a custom agent identifier
    let config = Config::infer().await?;

    let https = config.rustls_https_connector()?;

    let service = tower::ServiceBuilder::new()
        .layer(config.base_uri_layer())
        .option_layer(config.auth_layer()?)
        .service(hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build(https));

    let client = Client::new(service, config.default_namespace);

    Ok(client)
}
