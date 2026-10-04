//! Responsibility: runs the backing-track player's own output stream.
//!
//! The player is an independent pipeline (invariant #4): its own worker thread
//! renders the track, its own auxiliary output plays it, and the backend sums
//! that output with whatever else the device is playing. It never joins a
//! chain, so a chain rebuild cannot interrupt the track and the track can never
//! reach a guitar's buffers.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Result};

use engine::player::output::PLAYER_RING_SAMPLES;
use engine::player::settings::PlayerSettings;
use engine::player::shared::PlayerCell;
use engine::spsc::SpscRing;

use crate::aux_output::{open_aux_output, AuxOutputHandle};
use crate::player_render::player_render;
use crate::player_worker::{spawn_player_worker, PlayerDecoder, PlayerRequest, PlayerWorkerHandle};
use crate::ProjectRuntimeController;

/// Everything the controller holds for the player.
#[derive(Default)]
pub(crate) struct PlayerSlot {
    shared: PlayerCell,
    output: RefCell<Option<AuxOutputHandle>>,
    worker: RefCell<Option<PlayerWorkerHandle>>,
    track: RefCell<Option<PathBuf>>,
}

impl ProjectRuntimeController {
    /// The player's shared state, for the dispatcher and the UI.
    pub fn player_shared(&self) -> PlayerCell {
        Arc::clone(&self.player.shared)
    }

    /// Whether the player's output stream is open.
    pub fn player_active(&self) -> bool {
        self.player.output.borrow().is_some()
    }

    /// The track the worker holds, if any.
    pub fn player_track(&self) -> Option<PathBuf> {
        self.player.track.borrow().clone()
    }

    /// Hands `path` to the worker, paused at its start. Loading the file the
    /// worker already holds does nothing, so a start may always load first.
    /// Never opens a stream.
    pub fn load_player_track(&self, path: &Path, decode: PlayerDecoder) {
        if self.player.track.borrow().as_deref() == Some(path) {
            return;
        }
        if self.player.worker.borrow().is_none() {
            match spawn_player_worker(Arc::clone(&self.player.shared), decode) {
                Ok(worker) => *self.player.worker.borrow_mut() = Some(worker),
                Err(error) => {
                    log::warn!("[player] cannot start the worker thread: {error}");
                    self.player.shared.set_failed(true);
                    return;
                }
            }
        }
        self.player.shared.set_playing(false);
        *self.player.track.borrow_mut() = Some(path.to_path_buf());
        self.send(PlayerRequest::Load(path.to_path_buf()));
    }

    /// Plays the loaded track on `device_id`, opening the player's stream
    /// there unless it already plays to that endpoint.
    pub fn start_player(&self, device_id: &str, targets: &[usize]) -> Result<()> {
        if self.player.track.borrow().is_none() {
            bail!("no backing track loaded");
        }
        self.open_player_output(device_id, targets)?;
        self.player.shared.set_playing(true);
        Ok(())
    }

    /// Moves an open player to `device_id`. The stream that is sounding stays
    /// until the new one is open, so a failed move never silences the track.
    pub fn refresh_player_output(&self, device_id: &str, targets: &[usize]) -> Result<()> {
        if !self.player_active() {
            return Ok(());
        }
        self.open_player_output(device_id, targets)
    }

    pub fn pause_player(&self) {
        self.player.shared.set_playing(false);
    }

    /// Stops, rewinds and closes the player's stream.
    pub fn stop_player(&self) {
        self.player.shared.set_playing(false);
        self.player.shared.request_seek(0.0);
        let closed = self.player.output.borrow_mut().take();
        drop(closed);
        self.send(PlayerRequest::Detach);
    }

    pub fn seek_player(&self, seconds: f64) {
        self.player.shared.request_seek(seconds.max(0.0));
    }

    pub fn set_player_settings(&self, settings: PlayerSettings) {
        self.player.shared.set_settings(settings);
    }

    fn open_player_output(&self, device_id: &str, targets: &[usize]) -> Result<()> {
        if self
            .player
            .output
            .borrow()
            .as_ref()
            .is_some_and(|handle| handle.serves(device_id, targets))
        {
            return Ok(());
        }
        let ring = Arc::new(SpscRing::new(PLAYER_RING_SAMPLES, 0.0));
        let render = player_render(Arc::clone(&self.player.shared), Arc::clone(&ring));
        let handle = open_aux_output(&self.device_settings, device_id, targets, "player", render)?;
        let sample_rate = handle.sample_rate();
        // The previous stream goes only now, dropped outside the borrow.
        let previous = self.player.output.replace(Some(handle));
        drop(previous);
        self.send(PlayerRequest::Attach { ring, sample_rate });
        Ok(())
    }

    fn send(&self, request: PlayerRequest) {
        if let Some(worker) = self.player.worker.borrow().as_ref() {
            worker.send(request);
        }
    }
}

#[cfg(test)]
#[path = "player_stream_tests.rs"]
mod tests;
