//! Responsibility: reads the metronome state the dispatcher owns.
//!
//! Split out of `metronome_wiring` (#913). The lamp timer compares this
//! snapshot against what the knobs currently draw, once a frame, so a tempo or
//! timbre set from MCP or a MIDI CC follows on screen without any event
//! reaching the window. With no project open there is no dispatcher to ask —
//! and answering with a default would draw a metronome that does not exist.

use std::cell::RefCell;
use std::rc::Rc;

use application::metronome_state::MetronomeSnapshot;

use crate::state::ProjectSession;

/// The dispatcher's metronome state, or `None` with no project open.
pub(crate) fn metronome_snapshot(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
) -> Option<MetronomeSnapshot> {
    project_session
        .borrow()
        .as_ref()
        .map(|session| session.dispatcher.metronome_snapshot())
}

#[cfg(test)]
#[path = "metronome_read_tests.rs"]
mod tests;
