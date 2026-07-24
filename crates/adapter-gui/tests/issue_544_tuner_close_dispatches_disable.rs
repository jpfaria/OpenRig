//! Issue #544 — closing the tuner window leaves the tuner runtime
//! enabled in the background.
//!
//! Repro reported by the user:
//!   1. Open the tuner window.
//!   2. Press POWER → tuner activates, mute auto-engages.
//!   3. Close the tuner window.
//!   4. Tuner stays alive: pitch detection keeps polling, output stays
//!      muted under the hood.
//!   5. Reopen the tuner → UI shows power OFF (per commit b616bde13
//!      "default off on open"), out of sync with the still-running
//!      backend. The user has to toggle ON then OFF to actually stop it.
//!
//! Root cause hypothesis:
//!   `on_close_tuner` / `on_close_tuner_window` in
//!   `crates/adapter-gui/src/tuner_wiring.rs` hide the window without
//!   dispatching `SelectionCommand::SetTunerEnabled { enabled: false }`. The
//!   dispatcher / runtime stays `tuner = true`, while the Slint
//!   `tuner-enabled` property is reset to false on next open.
//!
//! Contract under test:
//!   The "close tuner window" intent — represented as a pure
//!   `tuner_close_commands()` helper — MUST include
//!   `SelectionCommand::SetTunerEnabled { enabled: false }` so the wiring can
//!   route the same commands through the dispatcher when the window
//!   closes. Auto-engaged mute (set on power-on) MUST also be released
//!   so closing mirrors the explicit power-off.
//!
//! This test is RED today: `adapter_gui::tuner_close` does not exist
//! yet. Fix forward by extracting the close intent into a pure helper
//! and wiring the two `on_close_*` callbacks to dispatch it.

use adapter_gui::tuner_close::tuner_close_commands;
use application::command::{Command, SelectionCommand};

#[test]
fn close_intent_disables_tuner() {
    let cmds = tuner_close_commands();
    assert!(
        cmds.iter()
            .any(|c| matches!(
                c,
                Command::Selection(SelectionCommand::SetTunerEnabled { enabled: false })
            )),
        "closing the tuner window must dispatch SetTunerEnabled(false); \
         got {cmds:?}"
    );
}

#[test]
fn close_intent_releases_auto_engaged_mute() {
    let cmds = tuner_close_commands();
    assert!(
        cmds.iter()
            .any(|c| matches!(
                c,
                Command::Selection(SelectionCommand::SetOutputMuted { muted: false })
            )),
        "closing the tuner window must release the mute that power-on \
         auto-engaged (commit b616bde13); got {cmds:?}"
    );
}
