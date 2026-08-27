//! Responsibility: resolves the per-OS location of the MIDI configuration files.
//!
//! Split out of `io_bindings.rs` (#873), which could not be described without
//! naming two jobs: where the MIDI files live is not the same concern as
//! persisting the I/O binding registry.

use anyhow::{Context, Result};

use crate::FilesystemStorage;

impl FilesystemStorage {
    /// Per-OS path of the **legacy** single-file MIDI mapping (`adapter-midi`,
    /// issue #22). macOS `~/Library/Application Support/OpenRig/midi-map.yaml`,
    /// Windows `%APPDATA%\OpenRig\midi-map.yaml`, Linux
    /// `~/.config/OpenRig/midi-map.yaml`. Never hardcoded — resolved like every
    /// other OpenRig config file. After #499 this file is migrated on first
    /// load into the system [`midi_profile_path`] + system [`midi_bindings_path`]
    /// and then deleted; this getter survives only for the migration path.
    pub fn midi_map_path() -> Result<std::path::PathBuf> {
        let base_dir = dirs::config_dir()
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(std::path::PathBuf::from)
                    .map(|home| home.join(".config"))
            })
            .context("failed to resolve user config directory")?;
        Ok(base_dir.join("OpenRig").join("midi-map.yaml"))
    }

    /// Per-OS path of the **MIDI device profile** (ADR 0003 / #499): which
    /// controller to listen to. System layer; never overridden by the project.
    /// macOS `~/Library/Application Support/OpenRig/midi-profile.yaml`,
    /// Windows `%APPDATA%\OpenRig\midi-profile.yaml`, Linux
    /// `~/.config/OpenRig/midi-profile.yaml`.
    pub fn midi_profile_path() -> Result<std::path::PathBuf> {
        let base_dir = dirs::config_dir()
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(std::path::PathBuf::from)
                    .map(|home| home.join(".config"))
            })
            .context("failed to resolve user config directory")?;
        Ok(base_dir.join("OpenRig").join("midi-profile.yaml"))
    }

    /// Per-OS path of the **system-wide MIDI bindings fallback** (ADR 0003 /
    /// #499). Used at resolve time when a project carries no `midi:` field;
    /// the shipped default ships as `examples/midi-map.default.yaml` and the
    /// system fallback overrides it when present. Same per-OS layout as the
    /// other config files.
    pub fn midi_bindings_path() -> Result<std::path::PathBuf> {
        let base_dir = dirs::config_dir()
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(std::path::PathBuf::from)
                    .map(|home| home.join(".config"))
            })
            .context("failed to resolve user config directory")?;
        Ok(base_dir.join("OpenRig").join("midi-bindings.yaml"))
    }
}
