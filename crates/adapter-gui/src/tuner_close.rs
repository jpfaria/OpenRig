//! Issue #544 — close-tuner intent as a pure helper.
//!
//! Closing the tuner window is the inverse of "power on" (commit
//! b616bde13): the tuner runtime stops and the auto-engaged mute is
//! released. This file owns the *Command intent* of that teardown so
//! it can be reused across every close path (in-app close button,
//! OS-level window-X) and exercised in isolation. The actual side
//! effects on the GUI (timer stop, session drop, Slint props) stay in
//! `tuner_wiring.rs`; this helper just answers "what does the dispatcher
//! need to hear?".
//!
//! Background: before the fix, none of the close paths dispatched
//! these Commands. Worse, the windowed mode had no in-app close button
//! (`show-close-button: false` in `tuner_window.slint`), so the
//! existing `on_close_tuner_window` callback never fired — the OS-X
//! click would just hide the window while the polling timer and the
//! auto-engaged mute stayed alive.

use application::command::Command;

/// Commands the dispatcher must receive when the tuner window closes.
///
/// Order is intentional: `SetTunerEnabled { enabled: false }` first so
/// any observer (MCP, MIDI, future gRPC) sees the tuner go off before
/// the mute is released, matching the semantics of an explicit power
/// off.
pub fn tuner_close_commands() -> Vec<Command> {
    vec![
        Command::SetTunerEnabled { enabled: false },
        Command::SetOutputMuted { muted: false },
    ]
}
