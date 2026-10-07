//! Responsibility: defines what the app asks of the TONE3000 API.
//!
//! The trait is the seam between the installer / browser and the network:
//! [`super::api_http::HttpTone3000Api`] talks to the real service, tests plug
//! an in-memory one in.

use std::path::Path;

use super::api_enums::Tone3000Architecture;
use super::api_types::{Model, Page, Tone};
use super::api_url::SearchQuery;

/// Paging stops here even if the API keeps answering: no tone has
/// anywhere near this many captures.
const MAX_MODEL_PAGES: u32 = 50;

#[derive(Debug, Clone, PartialEq)]
pub enum ApiError {
    /// The key is missing, wrong or revoked (401 / 403).
    InvalidKey,
    /// Too many requests (429); the user retries later.
    RateLimited,
    /// Any other non-success status.
    Http(u16),
    /// The service could not be reached.
    Network(String),
    /// A response did not have the expected shape.
    Parse(String),
    /// A local file could not be written.
    Io(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidKey => write!(f, "TONE3000 rejected the API key"),
            Self::RateLimited => write!(f, "TONE3000 rate limit reached; try again later"),
            Self::Http(status) => write!(f, "TONE3000 answered HTTP {status}"),
            Self::Network(e) => write!(f, "cannot reach TONE3000: {e}"),
            Self::Parse(e) => write!(f, "unexpected TONE3000 response: {e}"),
            Self::Io(e) => write!(f, "cannot write the download: {e}"),
        }
    }
}

impl std::error::Error for ApiError {}

/// Maps a non-success HTTP status to the error the user can act on.
pub fn status_error(status: u16) -> ApiError {
    match status {
        401 | 403 => ApiError::InvalidKey,
        429 => ApiError::RateLimited,
        other => ApiError::Http(other),
    }
}

/// The TONE3000 API v1 calls the app makes.
pub trait Tone3000Api: Send + Sync {
    fn search(&self, query: &SearchQuery) -> Result<Page<Tone>, ApiError>;
    fn tone(&self, id: u64) -> Result<Tone, ApiError>;
    fn models_page(
        &self,
        tone_id: u64,
        architecture: Option<Tone3000Architecture>,
        page: u32,
    ) -> Result<Page<Model>, ApiError>;
    /// Downloads `url` (a model's `model_url`) into the file `dest`.
    fn download(&self, url: &str, dest: &Path) -> Result<(), ApiError>;
}

/// Every model row of a tone, across all pages.
pub fn all_models(
    api: &dyn Tone3000Api,
    tone_id: u64,
    architecture: Option<Tone3000Architecture>,
) -> Result<Vec<Model>, ApiError> {
    let mut rows = Vec::new();
    for page in 1..=MAX_MODEL_PAGES {
        let current = api.models_page(tone_id, architecture, page)?;
        let empty = current.data.is_empty();
        rows.extend(current.data);
        if empty || page >= current.total_pages {
            break;
        }
    }
    Ok(rows)
}
