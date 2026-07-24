//! #14 — close-metronome intent as a pure helper, mirroring
//! [`crate::tuner_close`].
//!
//! Closing the window is a power off: the click stops and the dedicated output
//! stream closes. Keeping the *Command intent* here means every close path (the
//! panel's close button, the OS window-X) says the same thing to the dispatcher,
//! so MCP / MIDI / future gRPC observers see the metronome go off instead of the
//! adapter silently dropping the stream.
//!
//! The side effects on the GUI (timer stop, stream teardown, Slint props) stay
//! in `metronome_wiring.rs`; this helper only answers "what does the dispatcher
//! need to hear?".

use application::command::{Command, MetronomeCommand};

/// Commands the dispatcher must receive when the metronome window closes.
pub fn metronome_close_commands() -> Vec<Command> {
    vec![Command::Metronome(MetronomeCommand::SetMetronomeEnabled {
        enabled: false,
    })]
}
