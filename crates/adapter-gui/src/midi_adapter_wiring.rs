//! Opt-in MIDI/BLE-MIDI controller adapter wiring (issue #22 + #499). Wired
//! once from `run_desktop_app` when `--midi[=PATH]` is given.
//!
//! Mirrors the MCP wiring: a complementary input source on the live
//! instance. `adapter-midi` runs on its own thread and submits `Command`s
//! over an `application::bridge` channel; this drain timer services them on
//! the Slint event-loop thread — the same place GUI callbacks dispatch — so
//! a footswitch and the GUI share one `ProjectSession` with no lock and
//! zero audio-thread impact. The drained events run the *same* screen +
//! runtime refresh a GUI click does (`apply_events_to_ui`), so a footswitch
//! moves the screen exactly like the mouse.
//!
//! Map resolution follows ADR 0003 / #499. With `--midi` (no path), the
//! daemon receives the resolved view: device profile from the system layer,
//! bindings from the current project (when it carries `midi.bindings`),
//! else the system fallback file (`midi-bindings.yaml`), else the shipped
//! default (`examples/midi-map.default.yaml`). `--midi=PATH` keeps the
//! direct legacy-file load (no migration, no resolution) so explicit map
//! files still work for testing.

use anyhow::Result;
use infra_filesystem::midi_profile::MidiDeviceProfile;
use infra_filesystem::{detect_data_root, midi_migrate, FilesystemStorage};
use slint::{Timer, Weak};

use crate::chain_rig_nav_wiring::{apply_events_to_ui, ChainRigNavCtx};
use crate::cli::MidiMapArg;
use crate::AppWindow;

/// Spawn the MIDI adapter thread and return the drain `Timer` (bound for the
/// whole `window.run()` so it keeps firing). The `Result` surfaces a thread-
/// spawn failure or a map-resolution error.
pub(crate) fn wire(window: Weak<AppWindow>, ctx: ChainRigNavCtx, arg: MidiMapArg) -> Result<Timer> {
    let (bridge, drain) = application::bridge::channel();

    match arg {
        MidiMapArg::Default => {
            // #499: migrate the legacy `midi-map.yaml` once, then resolve.
            let legacy = FilesystemStorage::midi_map_path()?;
            let profile_path = FilesystemStorage::midi_profile_path()?;
            let bindings_path = FilesystemStorage::midi_bindings_path()?;
            if let Err(e) =
                midi_migrate::migrate_legacy_midi_map(&legacy, &profile_path, &bindings_path)
            {
                log::warn!("legacy midi-map.yaml migration failed: {e}");
            }

            let profile = MidiDeviceProfile::load(&profile_path)?;
            let project_bindings = ctx
                .project_session
                .borrow()
                .as_ref()
                .and_then(|s| s.rig.as_ref())
                .and_then(|rig| rig.borrow().midi.as_ref().map(|m| m.bindings.clone()));

            let shipped_default = detect_data_root().join("examples/midi-map.default.yaml");
            let map = adapter_midi::resolve_midi_map(
                project_bindings.as_deref(),
                &profile,
                &bindings_path,
                &shipped_default,
            )?;

            log::info!(
                "MIDI adapter listening (resolved: input={:?}, bindings={})",
                map.input,
                map.bindings.len()
            );

            // #513 / #493: hand the daemon the process-wide learn flag.
            // The drain loop below flips it from Event::MidiLearnStarted /
            // MidiLearnStopped, which the dispatcher fans out after the
            // GUI dispatches Command::Start/StopMidiLearn.
            let learn = adapter_midi::learn_state();
            std::thread::Builder::new()
                .name("openrig-midi".into())
                .spawn(move || {
                    if let Err(e) = adapter_midi::run_blocking_with_map(bridge, map, learn) {
                        log::error!("MIDI adapter stopped: {e}");
                    }
                })?;
        }
        MidiMapArg::Path(map_path) => {
            // Direct legacy-file load — no migration, no resolution.
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
