//! Catalog scanner — discovers track directories on disk.

use std::fs;
use std::path::Path;

use crate::entry::TrackEntry;
use crate::TracksError;

/// Scan `catalog_dir` and return every subdirectory that contains a
/// valid `meta.yaml`.
///
/// Behavior is forgiving: a missing catalog directory returns an empty
/// vector; entries without a meta file or with a malformed one are
/// silently skipped so the catalog stays usable when partially broken.
/// Per-entry failures are not surfaced — callers wanting to repair
/// broken entries iterate over [`std::fs::read_dir`] directly and use
/// [`TrackEntry::load`].
///
/// # Errors
///
/// - [`TracksError::Io`] when `catalog_dir` exists but reading it fails
///   for reasons other than absence (e.g. permission denied).
pub fn scan_catalog(catalog_dir: &Path) -> Result<Vec<TrackEntry>, TracksError> {
    if !catalog_dir.exists() {
        return Ok(Vec::new());
    }
    let read_dir = fs::read_dir(catalog_dir).map_err(|err| TracksError::Io {
        path: catalog_dir.to_path_buf(),
        reason: err.to_string(),
    })?;

    let mut entries = Vec::new();
    for item in read_dir {
        let item = match item {
            Ok(item) => item,
            Err(_) => continue,
        };
        let path = item.path();
        if !path.is_dir() {
            continue;
        }
        if let Ok(entry) = TrackEntry::load(&path) {
            entries.push(entry);
        }
    }
    Ok(entries)
}
