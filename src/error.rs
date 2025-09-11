use crate::k8s::client::UserAgentError;
use derive_more::From;
use k8s_openapi::serde_json;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, From)]
pub enum Error {
    #[from]
    UserAgentError(UserAgentError),

    #[from]
    Json(serde_json::Error),

    #[from]
    Kube(kube::Error),

    #[from]
    Infer(kube::config::InferConfigError),

    #[from]
    HttpHeader(hyper::http::Error),

    #[from]
    Io(std::io::Error),
}

impl core::fmt::Display for Error {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
        write!(fmt, "{self:?}")
    }
}

impl std::error::Error for Error {}
