//! Per-section wirings for the Settings screen (#513).
//!
//! Each submodule binds one section's Slint callbacks to `Command`
//! dispatches. The container page just forwards callbacks; the
//! section files own one feature surface each. Order: audio,
//! language, midi_devices, project_meta, midi_mapping.

pub mod audio;
pub mod language;
// added in later tasks: midi_devices, project_meta, midi_mapping
