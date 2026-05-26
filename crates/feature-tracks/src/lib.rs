//! Tracks catalog (issue #553).
//!
//! Owns the on-disk format and the in-memory data model for the
//! tracks/stems catalog. Lives outside `feature-stems` so the GUI,
//! engine, and application layers can depend on the catalog without
//! pulling in symphonia/rubato/ort.

mod entry;
mod meta;
mod playback;
mod scan;

pub use entry::{StemInfo, StemKind, TrackEntry};
pub use meta::{TrackId, TrackMeta};
pub use playback::MultiStemPlayer;
pub use scan::scan_catalog;

/// Errors raised by the tracks catalog layer.
#[derive(Debug, thiserror::Error)]
pub enum TracksError {
    /// Filesystem operation failed.
    #[error("io failure for `{path}`: {reason}")]
    Io {
        /// Path involved in the failure.
        path: std::path::PathBuf,
        /// Human-readable reason.
        reason: String,
    },

    /// `meta.yaml` is missing, malformed, or fails schema validation.
    #[error("meta.yaml problem in `{path}`: {reason}")]
    Meta {
        /// Track directory in which the meta lives.
        path: std::path::PathBuf,
        /// Human-readable reason.
        reason: String,
    },
}
