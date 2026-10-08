//! Responsibility: remembers which TONE3000 tone version a package holds.
//!
//! The browser writes a small file beside the manifest at install, so it
//! can tell later that the tone changed on TONE3000. The manifest
//! format stays untouched. A package without the file has no known version.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::api_types::Tone;
use super::manifest_build::CaptureFile;

pub const STAMP_FILE: &str = "tone3000_source.yaml";

#[derive(Debug, Serialize, Deserialize)]
struct Stamp {
    tone_id: u64,
    updated_at: Option<String>,
    /// The TONE3000 name of each capture file, by its path in the package.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    captures: BTreeMap<PathBuf, String>,
}

/// Records `tone`'s version in the package folder `dir`, with the
/// TONE3000 name of each downloaded capture file.
pub fn write_stamp(dir: &Path, tone: &Tone, files: &[CaptureFile]) -> std::io::Result<()> {
    let stamp = Stamp {
        tone_id: tone.id,
        updated_at: tone.updated_at.clone(),
        captures: files
            .iter()
            .map(|f| (f.file.clone(), f.name.clone()))
            .collect(),
    };
    let yaml = serde_yaml::to_string(&stamp).map_err(std::io::Error::other)?;
    std::fs::write(dir.join(STAMP_FILE), yaml)
}

/// The tone version the package in `dir` holds, when it is known.
pub fn read_updated_at(dir: &Path) -> Option<String> {
    read_stamp(dir)?.updated_at
}

/// The TONE3000 name of each capture file the package in `dir` holds,
/// keyed by its path inside the package. Empty when none was recorded.
pub fn read_capture_names(dir: &Path) -> BTreeMap<PathBuf, String> {
    read_stamp(dir).map(|s| s.captures).unwrap_or_default()
}

fn read_stamp(dir: &Path) -> Option<Stamp> {
    let text = std::fs::read_to_string(dir.join(STAMP_FILE)).ok()?;
    serde_yaml::from_str(&text).ok()
}

#[cfg(test)]
#[path = "source_stamp_tests.rs"]
mod tests;
