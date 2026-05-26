//! ID3/Vorbis/MP4 tag extraction via `lofty`.
//!
//! Best-effort, forgiving: any failure (missing file, unsupported
//! container, no embedded tags) collapses to `Default::default()` so
//! the orchestrator can still produce a track meta.yaml with empty
//! optional fields rather than aborting the whole separation.

use std::path::Path;

use lofty::file::TaggedFileExt;
use lofty::probe::Probe;
use lofty::tag::Accessor;

use crate::StemError;

/// Subset of audio file metadata the catalog cares about.
///
/// Every field is optional — sources without tags produce a fully
/// empty value, which is then stored as `null` in `meta.yaml`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExtractedTags {
    /// Track title.
    pub title: Option<String>,
    /// Lead artist.
    pub artist: Option<String>,
    /// Album name.
    pub album: Option<String>,
    /// Release year.
    pub year: Option<u32>,
    /// Genre.
    pub genre: Option<String>,
}

/// Extract the subset of tags relevant to the tracks catalog.
///
/// Returns [`ExtractedTags::default`] when the file cannot be opened
/// or has no tag block. This never errors on read paths — the
/// orchestrator must keep producing a track even when tags are
/// missing.
///
/// # Errors
///
/// The signature returns [`Result`] for forward compatibility (a future
/// version may want to surface fatal IO errors), but the current
/// implementation never produces an error.
pub fn extract_tags(path: &Path) -> Result<ExtractedTags, StemError> {
    let probe = match Probe::open(path) {
        Ok(probe) => probe,
        Err(_) => return Ok(ExtractedTags::default()),
    };
    let tagged = match probe.read() {
        Ok(t) => t,
        Err(_) => return Ok(ExtractedTags::default()),
    };
    let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) else {
        return Ok(ExtractedTags::default());
    };

    Ok(ExtractedTags {
        title: tag.title().map(|s| s.to_string()),
        artist: tag.artist().map(|s| s.to_string()),
        album: tag.album().map(|s| s.to_string()),
        year: tag.year(),
        genre: tag.genre().map(|s| s.to_string()),
    })
}
