//! Catalog entry: in-memory view of a track directory.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::meta::TrackMeta;
use crate::TracksError;

const META_FILENAME: &str = "meta.yaml";
const PEAKS_FILENAME: &str = "peaks.bin";

/// One of the canonical Demucs stems.
///
/// The variant order matches the model output order:
/// - `htdemucs` (4 stems): Drums, Bass, Vocals, Other
/// - `htdemucs_6s` (6 stems): Drums, Bass, Vocals, Other, Guitar, Piano
///
/// New variants stay at the END so existing 4-stem catalogs keep
/// indexing into the same positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StemKind {
    /// Drum kit / percussion.
    Drums,
    /// Bass guitar / synth bass / sub.
    Bass,
    /// Lead and backing vocals.
    Vocals,
    /// Everything not captured by the named stems.
    Other,
    /// Electric / acoustic guitar (htdemucs_6s only).
    Guitar,
    /// Piano / keys (htdemucs_6s only).
    Piano,
}

impl StemKind {
    /// Default filename used by the separation pipeline for this stem.
    #[must_use]
    pub fn default_filename(self) -> &'static str {
        match self {
            Self::Drums => "drums.wav",
            Self::Bass => "bass.wav",
            Self::Vocals => "vocals.wav",
            Self::Other => "other.wav",
            Self::Guitar => "guitar.wav",
            Self::Piano => "piano.wav",
        }
    }

    /// Canonical ordered list for a model that produces `count` stems.
    /// Returns an empty slice when no Demucs layout matches `count`.
    #[must_use]
    pub fn layout_for(count: usize) -> &'static [StemKind] {
        match count {
            4 => &[Self::Drums, Self::Bass, Self::Vocals, Self::Other],
            6 => &[
                Self::Drums,
                Self::Bass,
                Self::Vocals,
                Self::Other,
                Self::Guitar,
                Self::Piano,
            ],
            _ => &[],
        }
    }
}

/// One stem entry inside a [`TrackMeta`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StemInfo {
    /// Canonical stem kind.
    pub kind: StemKind,
    /// Filename relative to the track directory (typically
    /// `drums.wav`, `bass.wav`, `vocals.wav`, `other.wav`).
    pub filename: String,
}

/// A track directory loaded into memory.
///
/// Wraps a [`TrackMeta`] alongside the directory it was loaded from
/// so paths to stems and peaks can be resolved without re-discovering
/// the catalog root.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackEntry {
    /// Parsed `meta.yaml`.
    pub meta: TrackMeta,
    /// Track directory (the parent of `meta.yaml` and the stem WAVs).
    pub dir: PathBuf,
}

impl TrackEntry {
    /// Load a track entry from `dir`, parsing `dir/meta.yaml`.
    ///
    /// # Errors
    ///
    /// - [`TracksError::Meta`] when `meta.yaml` is missing or malformed.
    pub fn load(dir: &Path) -> Result<Self, TracksError> {
        let meta_path = dir.join(META_FILENAME);
        let raw = fs::read_to_string(&meta_path).map_err(|err| TracksError::Meta {
            path: dir.to_path_buf(),
            reason: format!("cannot read meta.yaml: {err}"),
        })?;
        let meta: TrackMeta = serde_yaml::from_str(&raw).map_err(|err| TracksError::Meta {
            path: dir.to_path_buf(),
            reason: format!("yaml: {err}"),
        })?;
        Ok(Self {
            meta,
            dir: dir.to_path_buf(),
        })
    }

    /// Persist `meta.yaml` inside [`Self::dir`]. The directory must
    /// already exist.
    ///
    /// # Errors
    ///
    /// - [`TracksError::Io`] when writing the file fails.
    /// - [`TracksError::Meta`] when serialising the meta fails.
    pub fn save(&self) -> Result<(), TracksError> {
        let yaml = serde_yaml::to_string(&self.meta).map_err(|err| TracksError::Meta {
            path: self.dir.clone(),
            reason: format!("serialise yaml: {err}"),
        })?;
        let meta_path = self.dir.join(META_FILENAME);
        fs::write(&meta_path, yaml).map_err(|err| TracksError::Io {
            path: meta_path,
            reason: err.to_string(),
        })?;
        Ok(())
    }

    /// Absolute path to the per-stem WAV file for `kind`.
    ///
    /// Looks up the recorded filename inside [`TrackMeta::stems`] and
    /// falls back to the canonical filename if the stem is not listed
    /// (defensive for partially-failed separations).
    #[must_use]
    pub fn stem_path(&self, kind: StemKind) -> PathBuf {
        let filename = self
            .meta
            .stems
            .iter()
            .find(|s| s.kind == kind)
            .map(|s| s.filename.as_str())
            .unwrap_or_else(|| kind.default_filename());
        self.dir.join(filename)
    }

    /// Absolute path to the pre-rendered waveform peaks file (legacy
    /// single-blob format).
    #[must_use]
    pub fn peaks_path(&self) -> PathBuf {
        self.dir.join(PEAKS_FILENAME)
    }

    /// Absolute path to a per-stem peak thumbnail PNG. The pipeline
    /// writes one of these next to each stem WAV when separation
    /// completes; the GUI renders them as the waveform under the
    /// stem strip controls.
    #[must_use]
    pub fn stem_peaks_path(&self, kind: StemKind) -> PathBuf {
        let basename = kind.default_filename().trim_end_matches(".wav");
        self.dir.join("peaks").join(format!("{basename}.png"))
    }
}
