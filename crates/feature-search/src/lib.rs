//! Tracks-catalog search index (#553).
//!
//! Single Tantivy index with a `type` discriminator field so the same
//! engine can later host presets, block models, and projects. The MVP
//! only indexes tracks.

mod index;
mod query;
mod schema;

pub use index::TracksIndex;
pub use query::{SearchHit, SearchQuery};

/// Errors raised by the search layer.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    /// Filesystem operation on the index directory failed.
    #[error("io failure: {0}")]
    Io(String),

    /// Tantivy reported an error opening, writing to, or querying the
    /// index.
    #[error("tantivy: {0}")]
    Tantivy(String),
}
