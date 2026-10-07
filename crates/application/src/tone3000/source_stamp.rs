//! Responsibility: remembers which TONE3000 tone version a package holds.
//!
//! The browser writes a small file beside the manifest at install, so it
//! can tell later that the tone changed on TONE3000 (#879). The manifest
//! format stays untouched. A package without the file has no known version.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::api_types::Tone;

pub const STAMP_FILE: &str = "tone3000_source.yaml";

#[derive(Debug, Serialize, Deserialize)]
struct Stamp {
    tone_id: u64,
    updated_at: Option<String>,
}

/// Records `tone`'s version in the package folder `dir`.
pub fn write_stamp(dir: &Path, tone: &Tone) -> std::io::Result<()> {
    let stamp = Stamp {
        tone_id: tone.id,
        updated_at: tone.updated_at.clone(),
    };
    let yaml = serde_yaml::to_string(&stamp).map_err(std::io::Error::other)?;
    std::fs::write(dir.join(STAMP_FILE), yaml)
}

/// The tone version the package in `dir` holds, when it is known.
pub fn read_updated_at(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join(STAMP_FILE)).ok()?;
    serde_yaml::from_str::<Stamp>(&text).ok()?.updated_at
}
