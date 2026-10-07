//! Responsibility: names why installing or removing a TONE3000 package failed.

use super::api_client::ApiError;

#[derive(Debug, Clone, PartialEq)]
pub enum InstallError {
    Api(ApiError),
    /// The tone has no downloadable capture of the requested kind.
    NoCaptures,
    AlreadyInstalled(String),
    NotInstalled(String),
    /// Not the id of a TONE3000 package; nothing outside the TONE3000
    /// root can be removed through it.
    InvalidPluginId(String),
    /// A capture could not be measured (broken model or IR file).
    Measure(String),
    /// The generated manifest was rejected by the loader's validation.
    Manifest(String),
    Io(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api(e) => write!(f, "{e}"),
            Self::NoCaptures => write!(f, "this tone has no downloadable captures"),
            Self::AlreadyInstalled(id) => write!(f, "{id} is already installed"),
            Self::NotInstalled(id) => write!(f, "{id} is not installed"),
            Self::InvalidPluginId(id) => write!(f, "`{id}` is not a TONE3000 plugin id"),
            Self::Measure(e) => write!(f, "cannot measure a capture: {e}"),
            Self::Manifest(e) => write!(f, "invalid plugin manifest: {e}"),
            Self::Io(e) => write!(f, "cannot write the plugin: {e}"),
        }
    }
}

impl std::error::Error for InstallError {}

impl From<ApiError> for InstallError {
    fn from(error: ApiError) -> Self {
        Self::Api(error)
    }
}

impl From<std::io::Error> for InstallError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}
