//! Responsibility: handles the backing-track player commands.
//!
//! The dispatcher owns the player: it validates the payload, remembers the
//! choice, persists what belongs to the machine (volume and output), applies
//! it to the frontend's audio runtime through
//! [`crate::runtime_control::RuntimeControl`], and only then reports the event.
//!
//! **Isolation.** The player is an independent pipeline with its own output
//! stream, summed by the backend. Nothing here addresses a chain, and the
//! endpoint it plays through travels as an opaque key.

use std::path::PathBuf;

use anyhow::{bail, Result};

use engine::player::settings::PlayerSettings;
use infra_filesystem::PlayerConfig;

use crate::app_config_persist::persist_player;
use crate::command::{Command, PlayerCommand};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::player_library::is_backing_track_file;

impl LocalDispatcher {
    pub(crate) fn handle_player(&self, cmd: Command) -> Result<Vec<Event>> {
        let Command::Player(cmd) = cmd else {
            unreachable!("handle_player received a non-player command: {cmd:?}");
        };
        match cmd {
            PlayerCommand::LoadPlayerTrack { path } => self.load_player_track(path),
            PlayerCommand::SetPlayerPlaying { playing } => self.set_player_playing(playing),
            PlayerCommand::StopPlayer => {
                if let Some(control) = self.runtime_control() {
                    control.stop_player();
                }
                self.player_state().borrow_mut().set_playing(false);
                Ok(vec![
                    Event::PlayerTransportChanged { playing: false },
                    Event::PlayerSeeked { seconds: 0.0 },
                ])
            }
            PlayerCommand::SeekPlayer { seconds } => {
                if !seconds.is_finite() {
                    bail!("seek position must be a number of seconds, got {seconds}");
                }
                let seconds = seconds.max(0.0);
                if let Some(control) = self.runtime_control() {
                    control.seek_player(seconds);
                }
                Ok(vec![Event::PlayerSeeked { seconds }])
            }
            PlayerCommand::SetPlayerVolume { volume } => {
                let stored = self.edit_player_settings(|s| s.volume = volume);
                let volume = stored.volume;
                self.persist_player_field(move |config| config.volume = volume);
                Ok(vec![settings_event(stored)])
            }
            PlayerCommand::SetPlayerSpeed { speed } => {
                let stored = self.edit_player_settings(|s| s.speed = speed);
                Ok(vec![settings_event(stored)])
            }
            PlayerCommand::SetPlayerSemitones { semitones } => {
                let stored = self.edit_player_settings(|s| s.semitones = semitones);
                Ok(vec![settings_event(stored)])
            }
            PlayerCommand::SetPlayerLoop {
                start_seconds,
                end_seconds,
            } => {
                let range = Some((start_seconds, end_seconds));
                let candidate = PlayerSettings {
                    loop_range: range,
                    ..self.player_state().borrow().settings()
                }
                .clamped();
                if candidate.loop_range != range {
                    bail!(
                        "a loop must run forward from a start at or after 0 \
                         ({start_seconds} → {end_seconds} is not one)"
                    );
                }
                let stored = self.edit_player_settings(|s| s.loop_range = range);
                Ok(vec![settings_event(stored)])
            }
            PlayerCommand::ClearPlayerLoop => {
                let stored = self.edit_player_settings(|s| s.loop_range = None);
                Ok(vec![settings_event(stored)])
            }
            PlayerCommand::SetPlayerOutput { device_id } => {
                self.player_state()
                    .borrow_mut()
                    .set_output_key(device_id.clone());
                let persisted = device_id.clone();
                self.persist_player_field(move |config| config.output_device = persisted);
                // Picking is not playing: a stopped player only opens there
                // next time.
                if let Some(control) = self.runtime_control() {
                    control.refresh_player_output(device_id.as_deref())?;
                }
                Ok(vec![Event::PlayerOutputChanged { device_id }])
            }
        }
    }

    fn load_player_track(&self, path: PathBuf) -> Result<Vec<Event>> {
        if !path.is_file() {
            bail!("no backing track at '{}'", path.display());
        }
        if !is_backing_track_file(&path) {
            bail!(
                "'{}' is not a supported audio file (expected one of {})",
                path.display(),
                crate::player_library::BACKING_TRACK_EXTENSIONS.join(", ")
            );
        }
        if let Some(control) = self.runtime_control() {
            control.load_player_track(&path)?;
        }
        self.player_state().borrow_mut().set_track(path.clone());
        Ok(vec![Event::PlayerTrackLoaded { path }])
    }

    fn set_player_playing(&self, playing: bool) -> Result<Vec<Event>> {
        // One borrow that ends BEFORE the runtime is touched: the frontend's
        // control reads the dispatcher back while it resolves the endpoint.
        let state = self.player_state();
        let (track, settings, output_key) = {
            let player = state.borrow();
            (
                player.track().map(PathBuf::from),
                player.settings(),
                player.output_key().map(str::to_string),
            )
        };
        if playing {
            let Some(track) = track else {
                bail!("load a backing track before playing");
            };
            if let Some(control) = self.runtime_control() {
                // `?`: a start the runtime refused made no sound, so the state
                // keeps saying the player is stopped.
                control.start_player(&track, settings, output_key.as_deref())?;
            }
        } else if let Some(control) = self.runtime_control() {
            control.pause_player();
        }
        state.borrow_mut().set_playing(playing);
        Ok(vec![Event::PlayerTransportChanged { playing }])
    }

    /// Clamp and store a settings edit, then hand the result to the runtime.
    fn edit_player_settings(&self, edit: impl FnOnce(&mut PlayerSettings)) -> PlayerSettings {
        let stored = self.player_state().borrow_mut().update_settings(edit);
        if let Some(control) = self.runtime_control() {
            control.set_player_settings(stored);
        }
        stored
    }

    /// No attached config path ⇒ no write, so a test dispatcher never reaches
    /// the user's real config.
    fn persist_player_field(&self, mutate: impl FnOnce(&mut PlayerConfig) + Send + 'static) {
        if let Some(path) = self.player_state().borrow().config_path() {
            persist_player(path, mutate);
        }
    }
}

fn settings_event(settings: PlayerSettings) -> Event {
    Event::PlayerSettingsChanged {
        volume: settings.volume,
        speed: settings.speed,
        semitones: settings.semitones,
        loop_start: settings.loop_range.map(|(start, _)| start),
        loop_end: settings.loop_range.map(|(_, end)| end),
    }
}

#[cfg(test)]
#[path = "local_dispatcher_player_tests.rs"]
mod tests;
