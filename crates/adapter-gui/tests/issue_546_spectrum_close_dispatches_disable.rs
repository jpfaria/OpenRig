//! Issue #546 — closing the spectrum window leaves the FFT polling
//! timer + per-output stream taps alive.
//!
//! Sibling of #544 (tuner). Same architectural gap: the windowed
//! `SpectrumWindow` renders with `show-close-button: false`, so the
//! `on_close_spectrum_window` callback never fires. No
//! `Window::on_close_requested` handler is wired either, so closing via
//! the OS chrome (X / Cmd-W) skips `teardown_session` entirely. CPU
//! keeps burning and the dispatcher never sees
//! `Event::SpectrumEnabledChanged { false }`.
//!
//! Unlike the tuner there is no auto-engaged mute to release on close
//! — spectrum is a passive analyzer. The close intent is therefore a
//! single command: tell the dispatcher (and every observer that listens
//! through it) that the analyzer went off.
//!
//! Contract under test:
//!   `spectrum_close_commands()` returns a Vec containing
//!   `SelectionCommand::SetSpectrumEnabled { enabled: false }` so every close
//!   path routes the same intent through the dispatcher.
//!
//! This test is RED today: `adapter_gui::spectrum_close` does not
//! exist. Fix forward by extracting the helper and wiring both the
//! explicit close callback and the OS-level `on_close_requested`
//! handler, mirroring the #544 fix.

use adapter_gui::spectrum_close::spectrum_close_commands;
use application::command::{Command, SelectionCommand};

#[test]
fn close_intent_disables_spectrum() {
    let cmds = spectrum_close_commands();
    assert!(
        cmds.iter()
            .any(|c| matches!(
                c,
                Command::Selection(SelectionCommand::SetSpectrumEnabled { enabled: false })
            )),
        "closing the spectrum window must dispatch SetSpectrumEnabled(false); \
         got {cmds:?}"
    );
}
