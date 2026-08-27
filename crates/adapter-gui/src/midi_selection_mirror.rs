//! Responsibility: mirrors the dispatcher's selection into the MIDI daemon's snapshot.
//!
//! Split out of `midi_adapter_wiring` (#913). The daemon runs on its own thread
//! and `LocalDispatcher` is `!Send`, so the authoritative selection cannot be
//! read from there. The drain tick copies it into a snapshot the daemon may
//! read instead — without this, a footswitch bound to "toggle the active chain"
//! acts on whatever was selected when the app started (#548/#591).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use application::SelectionState;

use crate::state::ProjectSession;

/// Copy the dispatcher's selection into `daemon_selection`.
///
/// Returns whether the snapshot was refreshed. `false` ⇒ nothing to mirror (no
/// project open) or a poisoned lock, and the caller skips this tick rather than
/// handing the daemon a half-written snapshot.
pub(crate) fn mirror_selection(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    daemon_selection: &Arc<RwLock<SelectionState>>,
) -> bool {
    let borrowed = project_session.borrow();
    let Some(session) = borrowed.as_ref() else {
        return false;
    };
    let source = session.dispatcher.selection_state();
    let Ok(snapshot) = source.read().map(|guard| guard.clone()) else {
        return false;
    };
    let Ok(mut destination) = daemon_selection.write() else {
        return false;
    };
    *destination = snapshot;
    true
}

#[cfg(test)]
#[path = "midi_selection_mirror_tests.rs"]
mod tests;
