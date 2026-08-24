//! Responsibility: keeps the audio backend connected across a backend loss.
//!
//! On Linux/JACK a device unplug makes udev restart jackd; the health timer
//! polls `is_healthy` and drives `try_reconnect`. On CoreAudio/WASAPI device
//! loss surfaces through stream error callbacks instead.

use anyhow::Result;

use project::project::Project;

use crate::controller::ProjectRuntimeController;

#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::host::using_jack_direct;
#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::jack_supervisor;
#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::usb_proc::detect_all_usb_audio_cards;

impl ProjectRuntimeController {
    /// Check whether the audio backend is still healthy.
    ///
    /// On Linux/JACK: returns false when the JACK server has disappeared (e.g.
    /// USB audio device unplugged → udev restarts jackd). The caller should
    /// tear down the runtime and attempt reconnection once JACK reappears.
    ///
    /// On macOS/Windows (CoreAudio/WASAPI): always returns true — device loss
    /// is detected through stream error callbacks, not polling.
    pub fn is_healthy(&mut self) -> bool {
        if self.active_chains.is_empty() {
            return true;
        }
        #[cfg(all(target_os = "linux", feature = "jack"))]
        if using_jack_direct() {
            // Delegate to the supervisor. health_check is non-destructive —
            // any verdict other than Healthy triggers the reconnect path in
            // the health timer (adapter-gui), which calls try_reconnect. The
            // next ensure_server fires a fresh spawn for any zombie or
            // not-running server.
            let verdicts = self.supervisor.health_check();
            return verdicts
                .values()
                .all(|v| matches!(v, jack_supervisor::HealthStatus::Healthy));
        }
        true
    }

    /// Attempt to reconnect after the audio backend became unhealthy.
    ///
    /// Tears down all active chains, forces the supervisor to stop every
    /// tracked jackd, and re-syncs the project. Returns Ok(true) if
    /// reconnection succeeded, Ok(false) if the backend is not yet available
    /// (no USB device).
    pub fn try_reconnect(&mut self, project: &Project) -> Result<bool> {
        log::info!("try_reconnect: checking if audio backend is available");

        #[cfg(all(target_os = "linux", feature = "jack"))]
        if using_jack_direct() && detect_all_usb_audio_cards().is_empty() {
            log::debug!("try_reconnect: no USB audio card found");
            return Ok(false);
        }

        // Tear down everything cleanly. On Linux this includes forcing the
        // supervisor to drop its tracked jackd — sync_project's ensure_server
        // then re-spawns with the desired config.
        self.stop();
        #[cfg(all(target_os = "linux", feature = "jack"))]
        if let Err(e) = self.supervisor.shutdown_all() {
            log::warn!("try_reconnect: supervisor.shutdown_all failed: {}", e);
        }

        match self.sync_project(project) {
            Ok(()) => {
                log::info!(
                    "try_reconnect: successfully reconnected with {} chains",
                    self.active_chains.len()
                );
                Ok(true)
            }
            Err(e) => {
                log::warn!("try_reconnect: sync_project failed: {}", e);
                Err(e)
            }
        }
    }
}
