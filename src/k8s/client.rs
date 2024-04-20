use crate::error::Result as NvResult;
use hyper::Request;
use hyper_util::rt::TokioExecutor;
use kube::{client::ConfigExt, Client, Config};
use pin_project::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{Layer, Service};

pub struct UserAgentLayer {
    user_agent: String,
}

impl UserAgentLayer {
    #[must_use]
    pub fn new(user_agent: &str) -> Self {
        Self {
            user_agent: user_agent.to_string(),
        }
    }
}

impl<S> Layer<S> for UserAgentLayer {
    type Service = UserAgentService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        UserAgentService {
            inner,
            user_agent: self.user_agent.clone(),
        }
    }
}

#[pin_project]
pub struct UserAgentService<S> {
    #[pin]
    inner: S,
    user_agent: String,
}

impl<S, ReqBody> Service<Request<ReqBody>> for UserAgentService<S>
where
    S: Service<Request<ReqBody>>,
    S::Response: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<ReqBody>) -> Self::Future {
        let header_value = hyper::header::HeaderValue::from_str(&self.user_agent)
            .unwrap_or_else(|_| hyper::header::HeaderValue::from_static("navipod"));

        req.headers_mut()
            .insert(hyper::header::USER_AGENT, header_value);

        let fut = self.inner.call(req);
        Box::pin(fut)
    }
}

/// Create a new k8s client to interact with k8s cluster api
///
/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
pub async fn new() -> NvResult<Client> {
    let config = Config::infer().await?;

    let https = config.rustls_https_connector()?;

    let service = tower::ServiceBuilder::new()
        .layer(UserAgentLayer::new("navipod/1.0")) // todo: manage via CICD
        .layer(config.base_uri_layer())
        .option_layer(config.auth_layer()?)
        .service(hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build(https));

    let client = Client::new(service, config.default_namespace);

    Ok(client)
}
