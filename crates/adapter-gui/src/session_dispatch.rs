//! Responsibility: dispatches a command without holding the session borrowed.
//!
//! Split out of `tuner_wiring` and `spectrum_wiring` (#913), which carried a
//! copy each. The rule it encodes is not incidental: dispatching APPLIES the
//! runtime effect, and a handler that reaches back for the session — powering
//! an analyzer, rebuilding a chain — would find the cell already borrowed and
//! panic. So the dispatcher handle is cloned out FIRST and the borrow dropped
//! before the command runs.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::Command;

use crate::state::ProjectSession;

/// Dispatch `command` against the open session, if there is one.
///
/// Returns whether a dispatcher took it. A failure is logged under `tag` and
/// reported as `false` — these are fire-and-forget UI actions, and the toggle
/// still has to reflect the user's press either way.
pub(crate) fn dispatch_detached(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    tag: &str,
    command: Command,
) -> bool {
    let dispatcher = project_session
        .borrow()
        .as_ref()
        .map(|session| session.dispatcher.clone());
    let Some(dispatcher) = dispatcher else {
        return false;
    };
    // The borrow above is gone by now — on purpose.
    if let Err(e) = dispatcher.dispatch(command) {
        log::warn!("[{tag}] command failed: {e}");
        return false;
    }
    true
}

#[cfg(test)]
#[path = "session_dispatch_tests.rs"]
mod tests;
