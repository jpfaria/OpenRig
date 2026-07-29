//! #436 G — `SelectionCommand::SetOutputMuted`: silencing/un-silencing the
//! output (tuner mute) is business, not UI. The dispatcher records the state
//! and signals `Event::OutputMutedChanged`, so MCP/MIDI/GUI all ask through
//! the same door. File-per-feature.
//!
//! #127: the audio effect landed here too. It used to be the caller's job
//! (`rt.set_output_muted` in a GUI callback), which meant a dispatch from MCP
//! or MIDI changed the flag and left the rig audible. The dispatcher now
//! applies it through the attached `RuntimeControl`; a transport that hosts
//! no runtime attaches none and the command is a pure state change.

use anyhow::Result;

use crate::command::{Command, SelectionCommand};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// `SelectionCommand::SetOutputMuted` — record the mute state, apply it to
    /// the frontend's audio runtime, and signal `Event::OutputMutedChanged`.
    pub(crate) fn handle_set_output_muted(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::Selection(SelectionCommand::SetOutputMuted { muted }) => {
                // #548: mirror into SelectionState so MIDI slot
                // `toggle_output_mute` reads the current state.
                if let Ok(mut s) = self.selection_state.write() {
                    s.output_muted = muted;
                }
                if let Some(control) = self.runtime_control.borrow().as_ref() {
                    control.set_output_muted(muted);
                }
                Ok(vec![Event::OutputMutedChanged { muted }])
            }
            other => {
                unreachable!("handle_set_output_muted received non-mute command: {other:?}")
            }
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_output_tests.rs"]
mod tests;
