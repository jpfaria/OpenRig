//! Responsibility: names the backing-track player commands.
//!
//! The player is a global utility, not part of any chain, and plays through
//! its own output stream. These commands carry intent; the dispatcher
//! validates and remembers them and the frontend that hosts the audio runtime
//! applies them.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Every backing-track player state change any controller can request.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum PlayerCommand {
    /// Load an audio file (wav, flac, mp3, ogg or m4a). It starts paused at
    /// the beginning.
    LoadPlayerTrack { path: PathBuf },

    /// Play or pause the loaded track. Pausing keeps the position.
    SetPlayerPlaying { playing: bool },

    /// Stop and rewind to the start.
    StopPlayer,

    /// Jump to `seconds` into the track.
    SeekPlayer { seconds: f64 },

    /// Track level, `0.0..=1.0`, independent of any chain volume.
    SetPlayerVolume { volume: f32 },

    /// Playback speed, `0.5..=2.0`. The pitch stays where it is.
    SetPlayerSpeed { speed: f32 },

    /// Transpose in semitones, `-12..=12`. The speed stays where it is.
    SetPlayerSemitones { semitones: f32 },

    /// Repeat the stretch between `start_seconds` and `end_seconds`.
    SetPlayerLoop {
        start_seconds: f64,
        end_seconds: f64,
    },

    /// Play the whole track again.
    ClearPlayerLoop,

    /// Delete a track from the user's own backing-track folder. The tracks
    /// that ship with the app and files anywhere else are refused. A loaded
    /// track that is deleted is stopped and unloaded.
    DeletePlayerTrack { path: PathBuf },

    /// Which output endpoint the track plays through. `None` falls back to
    /// the project's first output.
    SetPlayerOutput { device_id: Option<String> },
}
