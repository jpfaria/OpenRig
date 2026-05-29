//! Opt-in MIDI/BLE-MIDI controller adapter wiring (issue #548 supersedes
//! the earlier #22 + #499 path). Wired once from `run_desktop_app` when
//! `--midi[=PATH]` is given.
//!
//! - `--midi` (no path): profile-driven daemon. Loads every factory
//!   profile from `<data-root>/assets/midi-profiles/` plus every user
//!   profile from `<data-dir>/openrig/midi-profiles/` and routes
//!   incoming MIDI through them. See `docs/midi-profiles.md`.
//! - `--midi=PATH`: legacy single-file map loader, kept for testing an
//!   explicit `midi-map.yaml`. No profile resolution, no migration.
//!
//! `adapter-midi` runs on its own thread and submits `Command`s over an
//! `application::bridge` channel. The drain timer below services them
//! on the Slint event-loop thread — the same place GUI callbacks
//! dispatch — so a footswitch and the GUI share one `ProjectSession`
//! with no lock and zero audio-thread impact.

use std::sync::{Arc, RwLock};

use anyhow::Result;
use application::SelectionState;
use slint::{ComponentHandle, Timer, Weak};

use crate::chain_rig_nav_wiring::{apply_events_to_ui, ChainRigNavCtx};
use crate::cli::MidiMapArg;
use crate::AppWindow;

pub(crate) fn wire(window: Weak<AppWindow>, ctx: ChainRigNavCtx, arg: MidiMapArg) -> Result<Timer> {
    let (bridge, drain) = application::bridge::channel();
    // #548: the profile-driven daemon reads a snapshot of GUI selection
    // from this Arc<RwLock<_>>. The drain timer (below) copies the
    // dispatcher's authoritative `selection_state()` into it on every
    // tick so the daemon thread can read without crossing back to the
    // GUI thread — `LocalDispatcher` is !Send.
    let daemon_selection: Arc<RwLock<SelectionState>> =
        Arc::new(RwLock::new(SelectionState::default()));

    match arg {
        MidiMapArg::Default => {
            let learn = adapter_midi::learn_state();
            let _midi_thread =
                crate::start_midi_profiles(bridge.clone(), Arc::clone(&daemon_selection), learn);
        }
        MidiMapArg::Path(map_path) => {
            log::info!(
                "MIDI adapter listening (legacy map: {})",
                map_path.display()
            );
            let map_for_thread = map_path.clone();
            let learn = adapter_midi::learn_state();
            std::thread::Builder::new()
                .name("openrig-midi".into())
                .spawn(move || {
                    if let Err(e) = adapter_midi::run_blocking(bridge, &map_for_thread, learn) {
                        log::error!("MIDI adapter stopped: {e}");
                    }
                })?;
        }
    }

    let timer = Timer::default();
    let daemon_selection_for_timer = Arc::clone(&daemon_selection);
    // #591: the chain/block selection markers are MIDI-activity feedback —
    // shown only when a MIDI command arrives and hidden again after 10s of
    // no further MIDI. This one-shot timer is re-armed on every MIDI command.
    let markers_hide_timer: std::rc::Rc<std::cell::RefCell<Option<Timer>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(16),
        move || {
            // #548: mirror the authoritative selection state (kept on
            // the !Send `LocalDispatcher`) into the daemon's snapshot
            // so MIDI slots that read "active chain / active block"
            // see what the user has selected. Cheap clone — the
            // struct is a handful of `Option<String>` + bools.
            {
                let session_borrow = ctx.project_session.borrow();
                if let Some(session) = session_borrow.as_ref() {
                    let src = session.dispatcher.selection_state();
                    let snapshot = match src.read() {
                        Ok(g) => g.clone(),
                        Err(_) => return,
                    };
                    if let Ok(mut dst) = daemon_selection_for_timer.write() {
                        *dst = snapshot;
                    }
                }
            }
            // Drain on the Slint thread, then drop the session borrow
            // before refreshing (apply_events_to_ui re-borrows it).
            let events = {
                let session_borrow = ctx.project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    return;
                };
                drain.drain(session.dispatcher.as_ref(), 32)
            };
            if events.is_empty() {
                return;
            }
            if let Some(window) = window.upgrade() {
                // #591: a MIDI command arrived — light the selection markers
                // and (re)arm the 10s hide timer so they fade out after the
                // last footswitch stimulus.
                window.set_midi_selection_active(true);
                let weak_for_hide = window.as_weak();
                let hide = Timer::default();
                hide.start(
                    slint::TimerMode::SingleShot,
                    std::time::Duration::from_secs(10),
                    move || {
                        if let Some(w) = weak_for_hide.upgrade() {
                            w.set_midi_selection_active(false);
                        }
                    },
                );
                *markers_hide_timer.borrow_mut() = Some(hide);
                apply_events_to_ui(&window, &ctx, &events);
            }
        },
    );
    Ok(timer)
}
