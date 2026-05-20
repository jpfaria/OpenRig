//! System-level MIDI device profile (ADR 0003 / #499).
//!
//! The profile says **which** controller to listen to — substring match on
//! the input port name, just like the legacy `midi-map.yaml`'s `input:`
//! field. It belongs to the machine (the controller you plugged in), not to
//! any specific project, so it lives at the per-OS config path resolved by
//! [`crate::FilesystemStorage::midi_profile_path`].
//!
//! A missing file is valid — `input = None` means "use the system default
//! input port". The project never overrides this dimension: bindings travel
//! with the project, device selection does not.
//!
//! Bindings (the project layer) live inside `project.openrig` under
//! `RigProject.midi.bindings`; see `project::midi::RigProjectMidi`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Which MIDI controller to listen to. System-level — same value applies to
/// every project on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MidiDeviceProfile {
    /// Case-insensitive substring of the input port name to open. `None` ⇒
    /// use the system default input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
}

impl MidiDeviceProfile {
    /// Load the profile from `path`. A missing file yields the default empty
    /// profile (input = None) — that's a valid "use system default" state, not
    /// an error.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path)
            .with_context(|| format!("reading MIDI profile {}", path.display()))?;
        let profile: Self = serde_yaml::from_str(&raw)
            .with_context(|| format!("parsing MIDI profile {}", path.display()))?;
        Ok(profile)
    }

    /// Persist the profile to `path`. The parent directory is created if
    /// needed (same convention as the rest of `FilesystemStorage`).
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating MIDI profile dir {}", parent.display()))?;
        }
        let raw = serde_yaml::to_string(self)?;
        fs::write(path, raw).with_context(|| format!("writing MIDI profile {}", path.display()))?;
        Ok(())
    }
}
