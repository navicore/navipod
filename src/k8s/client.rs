// A hundred lines of code just to add a correct User-Agent header.
use crate::error::Result as NvResult;
use hyper::Request;
use hyper_util::rt::TokioExecutor;
use kube::{client::ConfigExt, Client, Config};
use pin_project::pin_project;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{Layer, Service};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const MODULE: &str = env!("CARGO_PKG_NAME");

#[derive(Debug)]
pub struct UserAgentError {
    message: String,
}

impl fmt::Display for UserAgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for UserAgentError {}

pub struct UserAgentLayer {
    user_agent: hyper::header::HeaderValue,
}

impl UserAgentLayer {
    /// # Errors
    ///
    /// Will return `Err` if Layer cannot be created due to invalid `user_agent` header value
    pub fn new(user_agent: &str) -> Result<Self, UserAgentError> {
        let header_value =
            hyper::header::HeaderValue::from_str(user_agent).map_err(|e| UserAgentError {
                message: format!("can not parse user_agent: {e}"),
            })?;

        Ok(Self {
            user_agent: header_value,
        })
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
    user_agent: hyper::header::HeaderValue,
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
        req.headers_mut()
            .insert(hyper::header::USER_AGENT, self.user_agent.clone());

        let fut = self.inner.call(req);
        Box::pin(fut)
    }
}

/// Create a new k8s client to interact with k8s cluster api that includes User-Agent header
///
/// # Errors
///
/// Will return `Err` if data can not be retrieved from k8s cluster api
pub async fn new(custom_user_agent: Option<&str>) -> NvResult<Client> {
    let config = Config::infer().await?;

    let https = config.rustls_https_connector()?;

    let default_user_agent = format!("{MODULE}/{VERSION}");
    let user_agent_str = custom_user_agent.unwrap_or(&default_user_agent);

    let service = tower::ServiceBuilder::new()
        .layer(UserAgentLayer::new(user_agent_str)?)
        .layer(config.base_uri_layer())
        .option_layer(config.auth_layer()?)
        .service(hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build(https));

    let client = Client::new(service, config.default_namespace);

    Ok(client)
}
