//! #553 — stem separation commands.
//!
//! The dispatcher records intent and emits the queued event. The
//! adapter listens, spawns the off-RT worker, and runs
//! `feature_stems::separate_track`. Same precedent as
//! `local_dispatcher_diagnostic` (`Set*Enabled`) and the system-side
//! MIDI commands: business logic = adapter / worker, dispatcher =
//! contract.

use anyhow::Result;

use crate::command::Command;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// `Command::SeparateStems` — record intent and signal the queued
    /// event. The actual decode → resample → separate → write happens
    /// off-RT in the adapter worker (see `feature_stems::separate_track`).
    pub(crate) fn handle_separate_stems(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::SeparateStems { source_path } => {
                Ok(vec![Event::StemJobQueued { source_path }])
            }
            other => {
                unreachable!("handle_separate_stems received non-stems command: {other:?}")
            }
        }
    }
}
