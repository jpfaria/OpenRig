//! Responsibility: handles the diagnostic commands.
//! #436 H / #127 — `SelectionCommand::SetTunerEnabled` / `SetSpectrumEnabled`:
//! powering the tuner and the spectrum analyzers.
//!
//! The dispatcher records the intention, APPLIES it to the frontend's analyzer
//! through `RuntimeControl`, and reports the event. Before #127 the second step
//! lived in the GUI's POWER callback, so the same command from `adapter-midi`'s
//! `toggle_tuner` / `toggle_spectrum` footswitch slots — or from an MCP client
//! — flipped the mirror in `SelectionState` and started no analyzer at all.
//! File-per-feature.

use anyhow::Result;

use crate::command::{Command, SelectionCommand};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// `SelectionCommand::SetTunerEnabled` / `SetSpectrumEnabled` — record the
    /// intention, power the analyzer, report the event.
    ///
    /// Neither is fallible and neither may wake audio: an analyzer READS what
    /// is already sounding, so with nothing hosted it subscribes to nothing and
    /// the reading stays empty (`running: false` over `openrig://tuner`).
    pub(crate) fn handle_diagnostic_enabled(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::Selection(SelectionCommand::SetTunerEnabled { enabled }) => {
                // #548: mirror into the SelectionState snapshot so MIDI
                // slot `toggle_tuner` sees the current state on the next
                // press.
                if let Ok(mut s) = self.selection_state.write() {
                    s.tuner_enabled = enabled;
                }
                if let Some(control) = self.runtime_control() {
                    control.set_tuner_running(enabled);
                }
                Ok(vec![Event::TunerEnabledChanged { enabled }])
            }
            Command::Selection(SelectionCommand::SetSpectrumEnabled { enabled }) => {
                if let Ok(mut s) = self.selection_state.write() {
                    s.spectrum_enabled = enabled;
                }
                if let Some(control) = self.runtime_control() {
                    control.set_spectrum_running(enabled);
                }
                Ok(vec![Event::SpectrumEnabledChanged { enabled }])
            }
            other => {
                unreachable!("handle_diagnostic_enabled received non-diagnostic command: {other:?}")
            }
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_diagnostic_tests.rs"]
mod tests;
