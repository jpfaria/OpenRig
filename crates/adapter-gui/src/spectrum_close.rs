//! Issue #546 — close-spectrum intent as a pure helper.
//!
//! Sibling of `tuner_close.rs` (#544). Closing the spectrum window is
//! the inverse of "power on", but spectrum is a passive analyzer — it
//! does not auto-engage mute on power-on, so there is nothing extra to
//! release on close. The intent is a single command: tell the
//! dispatcher (and every observer routed through it) that the analyzer
//! went off.
//!
//! Background: before the fix, none of the close paths dispatched a
//! `SetSpectrumEnabled { enabled: false }`. Worse, the windowed
//! `SpectrumWindow` renders with `show-close-button: false`, so the
//! existing `on_close_spectrum_window` callback never fires — the
//! OS-X click would just hide the window while the polling timer and
//! per-output stream taps stayed alive.

use application::command::Command;

/// Command the dispatcher must receive when the spectrum window closes.
pub fn spectrum_close_commands() -> Vec<Command> {
    vec![Command::SetSpectrumEnabled { enabled: false }]
}
