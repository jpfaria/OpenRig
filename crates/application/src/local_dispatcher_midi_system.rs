//! `LocalDispatcher` MIDI system command handler (issue #792 split).
//!
//! Single responsibility: the system-side MIDI commands (#513/#493). None
//! touch the project — device selection is per-machine (ADR 0003), learn-mode
//! is daemon state, and `PublishMidiEvent` is a passthrough. Each arm records
//! the intent via an `Event`; the adapter does the actual work.

use anyhow::Result;

use crate::command::Command;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// #513 / #493: system-side MIDI commands. None of these touch the
    /// project (MIDI device selection is per-machine / ADR 0003; learn-mode
    /// is daemon state; PublishMidiEvent is a passthrough of a raw event the
    /// daemon submits through the existing command bridge so the publishing
    /// dispatcher's fan-out remains the single transport). Each arm only
    /// records the intent via an `Event` — the adapter does the actual work
    /// (persist config.yaml, toggle learn-mode flag, route the event into
    /// the mapping editor).
    pub(crate) fn handle_midi_system(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::SaveMidiDevices { .. } => Ok(vec![Event::MidiDevicesSaved]),
            Command::StartMidiLearn => Ok(vec![Event::MidiLearnStarted]),
            Command::StopMidiLearn => Ok(vec![Event::MidiLearnStopped]),
            Command::PublishMidiEvent { source } => Ok(vec![Event::MidiEventReceived { source }]),
            other => {
                unreachable!("handle_midi_system received non-midi-system command: {other:?}")
            }
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_midi_system_tests.rs"]
mod tests;
