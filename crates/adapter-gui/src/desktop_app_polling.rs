//! Responsibility: runs the background timers of the desktop app.
//! Two background timers wired by `run_desktop_app` against the main window.
//!
//! * **Error poll** (200 ms) — installs any chain rebuild the control worker
//!   finished (#672) and drains the audio engine's bounded `BlockError` queue,
//!   surfacing the first message as a toast. The queue is lock-free and
//!   dropped from the audio thread when full, so the UI only sees a fraction
//!   during error storms — that's intentional.
//! * **Audio health check** (2 s) — when a runtime is running but the backend
//!   reports itself unhealthy (JACK server down, CoreAudio device removed),
//!   shows a "reconnecting" toast and asks for a reconnect until the backend
//!   recovers. Device hot-plug detection was deliberately moved out of this
//!   timer because polling `/proc/asound/cards` triggered scarlett2_notify
//!   freezes on the Orange Pi USB-C OTG port; the device list now refreshes
//!   only on demand.
//!
//! #127: this module holds NO audio backend. It reads through
//! `LiveSource` (the errors, the health) and writes through `RuntimeControl`
//! (installing a rebuild, reconnecting) — the same two seams every other
//! module uses. Neither write is a `Command`: nobody asks for either one (see
//! `runtime_health`), so the tick calls its own control instead of dispatching.
//!
//! Both timers are leaked with `std::mem::forget` so they live for the
//! whole application lifetime — there's no `drop(window)` path that needs
//! them stopped, and stopping them on close would race with the close
//! callback.

use std::cell::RefCell;
use std::rc::Rc;

use application::live_source::LiveSource;
use application::runtime_control::RuntimeControl;
use slint::{ComponentHandle, Timer};

use crate::helpers::{set_status_error, set_status_info, set_status_warning};
use crate::AppWindow;

pub(crate) fn start(
    window: &AppWindow,
    toast_timer: Rc<Timer>,
    live: Rc<dyn LiveSource>,
    control: Rc<dyn RuntimeControl>,
) {
    // Issue #778: this is the UI/AppKit (main) thread. Record it so a VST3 plugin
    // whose native editor is open, when dropped on the control worker, has its
    // teardown marshaled back here instead of crashing off the main thread.
    project::vst3_editor::mark_main_thread();
    // Error polling timer — drains block errors from the audio engine and shows toasts
    {
        let weak_window = window.as_weak();
        let toast_timer_for_errors = toast_timer.clone();
        let live_for_errors = live.clone();
        let control_for_errors = control.clone();
        let error_poll_timer = Timer::default();
        error_poll_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(200),
            move || {
                let Some(win) = weak_window.upgrade() else {
                    return;
                };
                // Issue #672: apply any off-thread chain rebuilds finished by the
                // control worker — swaps the live runtime in on the frontend tick
                // so the heavy build never blocked the UI.
                control_for_errors.apply_finished_rebuilds();
                // Issue #778: run any VST3 teardown the control worker deferred to
                // the main thread (dropping a plugin off-main crashes).
                project::vst3_editor::drain_deferred_vst3_teardowns();
                // A draining read: whatever this takes, nobody else will see.
                let Some(errors) = live_for_errors.block_errors() else {
                    return;
                };
                if let Some(first) = errors.first() {
                    set_status_error(
                        &win,
                        &toast_timer_for_errors,
                        rust_i18n::t!("status-plugin-error", msg = first.message).as_ref(),
                    );
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
        let disconnected = Rc::new(RefCell::new(false));
        let health_timer = Timer::default();
        health_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(2),
            move || {
                let Some(win) = weak_window.upgrade() else {
                    return;
                };

                // NOTE: device hot-plug detection moved OUT of the health timer.
                // Periodically polling /proc/asound/cards while the Scarlett 4th Gen
                // is on the USB-C OTG port triggers scarlett2_notify 0x20000000 and
                // freezes the device. The device list now refreshes only when the
                // user enters a UI surface that needs it (chain I/O editor, Settings,
                // configure-project) — see the refresh_input_devices call sites.
                let Some(health) = live.audio_health() else {
                    return;
                };
                if !health.running {
                    return;
                }
                let mut is_disconnected = disconnected.borrow_mut();

                if health.healthy {
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
                    set_status_warning(
                        &win,
                        &toast_timer_health,
                        &rust_i18n::t!("status-audio-disconnected"),
                    );
                    log::warn!("health check: audio backend unhealthy, will attempt reconnection");
                }

                // Try to reconnect
                match control.reconnect_audio() {
                    Ok(true) => {
                        *is_disconnected = false;
                        set_status_info(
                            &win,
                            &toast_timer_health,
                            &rust_i18n::t!("status-audio-reconnected"),
                        );
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
