//! Responsibility: resolves where the MIDI configuration files live.
//!
//! Kept apart from `io_bindings.rs`: where the MIDI files live is not the same
//! concern as persisting the I/O binding registry. Every file sits at the root
//! of the user folder ([`crate::user_data_root`]).

use anyhow::Result;

use crate::{user_data_root, FilesystemStorage};

impl FilesystemStorage {
    /// Path of the **legacy** single-file MIDI mapping (`midi-map.yaml`). It is
    /// migrated on first load into the system [`Self::midi_profile_path`] +
    /// system [`Self::midi_bindings_path`] and then deleted; this getter
    /// survives only for the migration path.
    pub fn midi_map_path() -> Result<std::path::PathBuf> {
        Ok(user_data_root().join("midi-map.yaml"))
    }

    /// Path of the **MIDI device profile** (ADR 0003): which controller to
    /// listen to. System layer; never overridden by the project.
    pub fn midi_profile_path() -> Result<std::path::PathBuf> {
        Ok(user_data_root().join("midi-profile.yaml"))
    }

    /// Path of the **system-wide MIDI bindings fallback** (ADR 0003). Used at
    /// resolve time when a project carries no `midi:` field; the shipped
    /// default ships as `examples/midi-map.default.yaml` and the system
    /// fallback overrides it when present.
    pub fn midi_bindings_path() -> Result<std::path::PathBuf> {
        Ok(user_data_root().join("midi-bindings.yaml"))
    }
}
