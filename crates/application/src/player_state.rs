//! Responsibility: holds the backing-track player's control-plane state.
//!
//! The dispatcher owns it so every transport (GUI, MCP, MIDI) shares one truth
//! about which track is loaded and how it plays. The chosen output travels as
//! an opaque endpoint key, exactly as the metronome's does: only the frontend
//! that owns the audio host knows which device it names.

use std::path::{Path, PathBuf};

use engine::player::settings::PlayerSettings;
use infra_filesystem::PlayerConfig;

use crate::player_library::PlayerLibraryDirs;

/// What the player is set to, as a frontend renders it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerSnapshot {
    pub track: Option<PathBuf>,
    pub settings: PlayerSettings,
    /// The chosen output endpoint key, `None` for "the project's first".
    pub output_key: Option<String>,
    /// Whether the user asked for playback. Not persisted: the app always
    /// opens silent.
    pub playing: bool,
}

/// The dispatcher's player state.
#[derive(Debug, Default)]
pub struct PlayerControlState {
    snapshot: PlayerSnapshot,
    library: PlayerLibraryDirs,
    /// The per-machine `config.yaml` the volume and output persist to. `None`
    /// persists nothing, which keeps tests off the user's real config.
    config_path: Option<PathBuf>,
}

impl PlayerControlState {
    /// The state a frontend restores at boot.
    pub fn restored(
        config: &PlayerConfig,
        library: PlayerLibraryDirs,
        config_path: Option<PathBuf>,
    ) -> Self {
        let settings = PlayerSettings {
            volume: config.volume,
            ..PlayerSettings::default()
        }
        .clamped();
        Self {
            snapshot: PlayerSnapshot {
                settings,
                output_key: config.output_device.clone(),
                ..PlayerSnapshot::default()
            },
            library,
            config_path,
        }
    }

    pub fn config_path(&self) -> Option<PathBuf> {
        self.config_path.clone()
    }

    pub fn snapshot(&self) -> PlayerSnapshot {
        self.snapshot.clone()
    }

    pub fn settings(&self) -> PlayerSettings {
        self.snapshot.settings
    }

    pub fn track(&self) -> Option<&Path> {
        self.snapshot.track.as_deref()
    }

    /// A new track starts paused at its beginning, and the old loop does not
    /// carry over to it.
    pub fn set_track(&mut self, track: PathBuf) {
        self.snapshot.track = Some(track);
        self.snapshot.playing = false;
        self.snapshot.settings.loop_range = None;
    }

    pub fn output_key(&self) -> Option<&str> {
        self.snapshot.output_key.as_deref()
    }

    pub fn set_output_key(&mut self, key: Option<String>) {
        self.snapshot.output_key = key;
    }

    pub fn playing(&self) -> bool {
        self.snapshot.playing
    }

    pub fn set_playing(&mut self, playing: bool) {
        self.snapshot.playing = playing;
    }

    /// Edit the settings, keep them in range and hand back what was stored.
    pub fn update_settings(&mut self, edit: impl FnOnce(&mut PlayerSettings)) -> PlayerSettings {
        let mut settings = self.snapshot.settings;
        edit(&mut settings);
        self.snapshot.settings = settings.clamped();
        self.snapshot.settings
    }

    pub fn library(&self) -> PlayerLibraryDirs {
        self.library.clone()
    }

    pub fn set_user_dir(&mut self, dir: Option<PathBuf>) {
        self.library.user = dir;
    }
}

#[cfg(test)]
#[path = "player_state_tests.rs"]
mod tests;
