//! Responsibility: reaches the TONE3000 browser through the project's dispatcher.
//!
//! With no project open there is no dispatcher: a read answers `None` and a
//! command is dropped, never a browser that does not exist (#879).

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, Tone3000Command};
use application::event::Event;
use application::tone3000_state::Tone3000Snapshot;

use crate::state::ProjectSession;

pub(crate) type SessionCell = Rc<RefCell<Option<ProjectSession>>>;

/// The dispatcher's TONE3000 state, cheap enough to compare every tick.
pub(crate) fn tone3000_snapshot(session: &SessionCell) -> Option<Tone3000Snapshot> {
    session
        .borrow()
        .as_ref()
        .map(|s| s.dispatcher.tone3000_snapshot())
}

/// Send one command. `Err` carries the refusal the window shows; no project
/// is `Ok`, as there is nothing to tell.
pub(crate) fn dispatch_tone3000(
    session: &SessionCell,
    command: Tone3000Command,
) -> Result<(), String> {
    let borrowed = session.borrow();
    let Some(s) = borrowed.as_ref() else {
        return Ok(());
    };
    s.dispatcher
        .dispatch(Command::Tone3000(command))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Apply the answers of searches and installs that finished off-thread.
/// Returns every event the drain produced, whatever its domain, so the
/// caller can pass them on.
pub(crate) fn poll_tone3000(session: &SessionCell) -> Vec<Event> {
    session
        .borrow()
        .as_ref()
        .map(|s| s.dispatcher.poll_async_results())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "tone3000_session_tests.rs"]
mod tests;
