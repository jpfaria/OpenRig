//! Opt-in MIDI/BLE-MIDI controller adapter wiring (issue #22). Wired once
//! from `run_desktop_app` when `--midi[=PATH]` is given.
//!
//! Mirrors the MCP wiring: a complementary input source on the live
//! instance. `adapter-midi` runs on its own thread and submits `Command`s
//! over an `application::bridge` channel; this drain timer services them on
//! the Slint event-loop thread — the same place GUI callbacks dispatch — so
//! a footswitch and the GUI share one `ProjectSession` with no lock and
//! zero audio-thread impact. The drained events run the *same* screen +
//! runtime refresh a GUI click does (`apply_events_to_ui`), so a footswitch
//! moves the screen exactly like the mouse.

use std::path::PathBuf;

use anyhow::Result;
use slint::{Timer, Weak};

use crate::chain_rig_nav_wiring::{apply_events_to_ui, ChainRigNavCtx};
use crate::AppWindow;

/// Spawn the MIDI adapter thread and return the drain `Timer` (bound for the
/// whole `window.run()` so it keeps firing). `None` is never returned on the
/// happy path; the `Result` surfaces a thread-spawn failure.
pub(crate) fn wire(
    window: Weak<AppWindow>,
    ctx: ChainRigNavCtx,
    map_path: PathBuf,
) -> Result<Timer> {
    let (bridge, drain) = application::bridge::channel();
    let map_for_thread = map_path.clone();
    std::thread::Builder::new()
        .name("openrig-midi".into())
        .spawn(move || {
            if let Err(e) = adapter_midi::run_blocking(bridge, &map_for_thread) {
                log::error!("MIDI adapter stopped: {e}");
            }
        })?;
    log::info!("MIDI adapter listening (map: {})", map_path.display());

    let timer = Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(16),
        move || {
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
                apply_events_to_ui(&window, &ctx, &events);
            }
        },
    );
    Ok(timer)
}
