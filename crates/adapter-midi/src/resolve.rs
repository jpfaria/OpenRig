//! ADR 0003 / #499 MIDI map resolver — combines the project's MIDI bindings,
//! the system device profile, the system bindings fallback file, and the
//! shipped default into the single [`MidiMap`] the daemon consumes.
//!
//! Precedence at resolve time:
//!
//! - `input`: always from the system device profile. The project never
//!   overrides which controller to listen to — that's hardware-bound.
//! - `bindings`: project (when present and non-empty) → system fallback
//!   (`midi-bindings.yaml`, when the file exists) → shipped default
//!   (`examples/midi-map.default.yaml`, when the file exists) → empty.
//!
//! The assembled map is **validated** before being handed back: an unknown
//! `Command` or schema-violating args fails the resolve loudly, matching
//! `MidiMap::load`'s existing contract (issue #22).

use std::path::Path;

use anyhow::Result;
use infra_filesystem::midi_profile::MidiDeviceProfile;

use crate::mapping::{Binding, MidiMap};

/// Resolve the runtime [`MidiMap`] consumed by [`crate::run_blocking`].
///
/// See the module docs for the precedence rule. `project_bindings = None` or
/// `Some(&[])` both mean "no project layer" and fall through to the system
/// fallback. The two file paths may point to missing files — that's normal
/// and not an error.
pub fn resolve_midi_map(
    project_bindings: Option<&[Binding]>,
    profile: &MidiDeviceProfile,
    system_fallback_path: &Path,
    shipped_default_path: &Path,
) -> Result<MidiMap> {
    let bindings = if let Some(bs) = project_bindings.filter(|b| !b.is_empty()) {
        bs.to_vec()
    } else if system_fallback_path.exists() {
        load_bindings_only(system_fallback_path)?
    } else if shipped_default_path.exists() {
        load_bindings_only(shipped_default_path)?
    } else {
        Vec::new()
    };

    let map = MidiMap {
        input: profile.input.clone(),
        bindings,
    };
    map.validate()?;
    Ok(map)
}

/// Read a YAML file whose top-level shape is `{ bindings: [...] }` and
/// return just the `bindings` list. The legacy `MidiMap` shape (with an
/// optional `input:`) is accepted too — that field is ignored, because
/// the runtime `input` is always taken from the system device profile.
fn load_bindings_only(path: &Path) -> Result<Vec<Binding>> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("reading bindings file {}: {e}", path.display()))?;
    let map: MidiMap = serde_yaml::from_str(&raw)
        .map_err(|e| anyhow::anyhow!("parsing bindings file {}: {e}", path.display()))?;
    Ok(map.bindings)
}
