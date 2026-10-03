//! Responsibility: names every observable change of the backing-track player.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum PlayerEvent {
    /// A backing track was loaded into the player; it starts paused.
    TrackLoaded { path: PathBuf },

    /// A track was deleted from the user's backing-track folder.
    TrackDeleted { path: PathBuf },

    /// The player started or paused.
    TransportChanged { playing: bool },

    /// The player moved to a position, in seconds of the track.
    Seeked { seconds: f64 },

    /// The player's settings changed. Values are the applied, clamped ones.
    SettingsChanged {
        volume: f32,
        speed: f32,
        semitones: f32,
        loop_start: Option<f64>,
        loop_end: Option<f64>,
    },

    /// The player's output endpoint key changed.
    OutputChanged { device_id: Option<String> },
}
