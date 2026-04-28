//! Two background timers wired by `run_desktop_app` against the main window.
//!
//! * **Error poll** (200 ms) — drains the audio engine's bounded
//!   `BlockError` queue and surfaces the first message as a toast. The queue
//!   is lock-free and dropped from the audio thread when full, so the UI
//!   only sees a fraction during error storms — that's intentional.
//! * **Audio health check** (2 s) — when a runtime is running but
//!   `is_healthy()` reports false (JACK server down, CoreAudio device
//!   removed), shows a "reconnecting" toast and calls `try_reconnect` on the
//!   active project until the backend recovers. Device hot-plug detection
//!   was deliberately moved out of this timer because polling
//!   `/proc/asound/cards` triggered scarlett2_notify freezes on the Orange
//!   Pi USB-C OTG port; the device list now refreshes only on demand.
//!
//! Both timers are leaked with `std::mem::forget` so they live for the
//! whole application lifetime — there's no `drop(window)` path that needs
//! them stopped, and stopping them on close would race with the close
//! callback.

use std::cell::RefCell;
use std::rc::Rc;

use infra_cpal::ProjectRuntimeController;
use slint::{ComponentHandle, Timer};

use crate::helpers::{set_status_error, set_status_info, set_status_warning};
use crate::state::ProjectSession;
use crate::AppWindow;

pub(crate) fn start(
    window: &AppWindow,
    toast_timer: Rc<Timer>,
    project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
) {
    // Error polling timer — drains block errors from the audio engine and shows toasts
    {
        let weak_window = window.as_weak();
        let toast_timer_for_errors = toast_timer.clone();
        let project_runtime_for_errors = project_runtime.clone();
        let error_poll_timer = Timer::default();
        error_poll_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(200),
            move || {
                let Some(win) = weak_window.upgrade() else { return; };
                let rt_borrow = project_runtime_for_errors.borrow();
                let Some(rt) = rt_borrow.as_ref() else { return; };
                let errors = rt.poll_errors();
                if let Some(first) = errors.first() {
                    set_status_error(&win, &toast_timer_for_errors, &format!("Plugin error: {}", first.message));
                }
            },
        );
        std::mem::forget(error_poll_timer);
    }

    // Audio health check timer — detects device disconnects (JACK server
    // down on Linux, CoreAudio device removed on macOS) and auto-reconnects
    // when the backend becomes available again.
    {
        let weak_window = window.as_weak();
        let toast_timer_health = toast_timer;
        let runtime_health = project_runtime;
        let session_health = project_session;
        let disconnected = Rc::new(RefCell::new(false));
        let health_timer = Timer::default();
        health_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(2),
            move || {
                let Some(win) = weak_window.upgrade() else { return; };

                // NOTE: device hot-plug detection moved OUT of the health timer.
                // Periodically polling /proc/asound/cards while the Scarlett 4th Gen
                // is on the USB-C OTG port triggers scarlett2_notify 0x20000000 and
                // freezes the device. The device list now refreshes only when the
                // user enters a UI surface that needs it (chain I/O editor, Settings,
                // configure-project) — see the refresh_input_devices call sites.
                let mut rt_borrow = runtime_health.borrow_mut();
                let Some(rt) = rt_borrow.as_mut() else { return; };
                if !rt.is_running() {
                    return;
                }
                let mut is_disconnected = disconnected.borrow_mut();

                if rt.is_healthy() {
                    if *is_disconnected {
                        // Was disconnected, now healthy again — nothing to do,
                        // reconnection already happened
                        *is_disconnected = false;
                    }
                    return;
                }

                // Backend is unhealthy
                if !*is_disconnected {
                    *is_disconnected = true;
                    set_status_warning(&win, &toast_timer_health, "Audio device disconnected — reconnecting...");
                    log::warn!("health check: audio backend unhealthy, will attempt reconnection");
                }

                // Try to reconnect
                let session_borrow = session_health.borrow();
                let Some(session) = session_borrow.as_ref() else { return; };
                match rt.try_reconnect(&session.project) {
                    Ok(true) => {
                        *is_disconnected = false;
                        set_status_info(&win, &toast_timer_health, "Audio device reconnected");
                        log::info!("health check: successfully reconnected");
                    }
                    Ok(false) => {
                        log::debug!("health check: backend not ready yet, will retry");
                    }
                    Err(e) => {
                        log::warn!("health check: reconnection attempt failed: {}", e);
                    }
                }
            },
        );
        std::mem::forget(health_timer);
    }
}
