//! Responsibility: decides what one audio-health tick has to report.
//!
//! Split out of `desktop_app_polling` (#913). Showing a toast is screen work;
//! deciding WHEN there is something to say is a small state machine over the
//! backend's health and what the previous tick already reported — announce the
//! disconnect once, keep retrying quietly, announce the recovery once.

use std::cell::RefCell;

use application::live_source::LiveSource;
use application::runtime_control::RuntimeControl;

/// What the tick wants the window to show. Both flags false is by far the
/// common case: the timer runs every 2 s for the life of the app.
///
/// Both can be true on the same tick — the backend went away and the very
/// first retry brought it straight back, which the user sees as the warning
/// followed by the recovery.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct HealthReport {
    /// The backend just went away. Set once, not on every retry.
    pub(crate) announce_disconnect: bool,
    /// The backend came back on this tick.
    pub(crate) announce_reconnect: bool,
}

impl HealthReport {
    fn nothing() -> Self {
        Self::default()
    }
}

/// Run one health tick. `disconnected` carries the previous tick's verdict.
///
/// A rig that is not running has no backend to be unhealthy about, so it is
/// never reported — otherwise closing a project would raise a disconnect toast.
pub(crate) fn health_tick(
    live: &dyn LiveSource,
    control: &dyn RuntimeControl,
    disconnected: &RefCell<bool>,
) -> HealthReport {
    let Some(health) = live.audio_health() else {
        return HealthReport::nothing();
    };
    if !health.running {
        return HealthReport::nothing();
    }
    if health.healthy {
        *disconnected.borrow_mut() = false;
        return HealthReport::nothing();
    }

    let first_notice = !*disconnected.borrow();
    if first_notice {
        *disconnected.borrow_mut() = true;
        log::warn!("health check: audio backend unhealthy, will attempt reconnection");
    }

    let announce_reconnect = match control.reconnect_audio() {
        Ok(true) => {
            *disconnected.borrow_mut() = false;
            log::info!("health check: successfully reconnected");
            true
        }
        Ok(false) => {
            log::debug!("health check: backend not ready yet, will retry");
            false
        }
        Err(e) => {
            log::warn!("health check: reconnection attempt failed: {e}");
            false
        }
    };
    HealthReport {
        announce_disconnect: first_notice,
        announce_reconnect,
    }
}

#[cfg(test)]
#[path = "audio_health_tick_tests.rs"]
mod tests;
