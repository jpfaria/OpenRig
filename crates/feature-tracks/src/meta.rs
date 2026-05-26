//! `meta.yaml` data model.

use serde::{Deserialize, Serialize};

use crate::entry::StemInfo;

/// Stable identifier for a track inside the catalog.
///
/// Treated as opaque by consumers — the only contract is that it is a
/// non-empty string suitable for use as a directory name on every
/// supported platform. The Tracks pipeline generates a ULID-shaped
/// value at separation time, but tests may pass any short string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TrackId(String);

impl TrackId {
    /// Wrap an existing string into a [`TrackId`]. The value is taken
    /// as-is; no normalisation, case folding, or character validation.
    #[must_use]
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    /// Borrow the underlying string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for TrackId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Persistent metadata for a track inside the catalog.
///
/// Layout on disk: `<catalog>/<id>/meta.yaml` with this struct as
/// payload. Optional fields are omitted from the YAML when `None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackMeta {
    /// Stable id matching the directory name.
    pub id: TrackId,
    /// Human-readable title.
    pub title: String,
    /// Optional artist name (auto-filled from ID3/Vorbis tags).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    /// Optional album name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    /// Optional release year.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    /// Optional genre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    /// Optional BPM (beats per minute), as a float to allow halftime/
    /// non-integer estimates from analysis tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpm: Option<f32>,
    /// Optional musical key (e.g. `Am`, `F#`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Total duration in seconds.
    pub duration_secs: f64,
    /// Sample rate of the source audio and of the per-stem WAV files.
    pub source_sample_rate: u32,
    /// Stems produced by the separation step.
    pub stems: Vec<StemInfo>,
    /// Identifier of the model used to separate (e.g. `htdemucs`).
    pub model: String,
    /// ISO 8601 timestamp of when the stems were produced.
    pub generated_at: String,
}
