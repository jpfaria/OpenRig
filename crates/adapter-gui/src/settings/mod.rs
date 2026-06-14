//! Per-section wirings for the Settings screen (#513).
//!
//! Each submodule binds one section's Slint callbacks to `Command`
//! dispatches. The container page just forwards callbacks; the
//! section files own one feature surface each. Order: audio,
//! language, midi_devices, project_meta. (midi_mapping removed by
//! #548 — the legacy single-file binding editor was dead UI once the
//! profile-driven daemon shipped.)

pub mod audio;
pub mod integrations;
pub mod language;
pub mod midi_devices;
pub mod paths;
pub mod project_meta;
