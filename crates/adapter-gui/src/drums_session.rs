//! Responsibility: reaches the drum machine through the project's dispatcher.
//!
//! With no project open there is no dispatcher, so every read answers `None`
//! and every command is dropped — never a drum machine that does not exist.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, DrumsCommand};
use application::drums_state::DrumsSnapshot;

use crate::drums_view::{drums_view, DrumsView};
use crate::metronome_view::output_endpoints;
use crate::state::ProjectSession;

pub(crate) type SessionCell = Rc<RefCell<Option<ProjectSession>>>;

/// The dispatcher's drums state, cheap enough to compare every frame.
pub(crate) fn drums_snapshot(session: &SessionCell) -> Option<DrumsSnapshot> {
    session
        .borrow()
        .as_ref()
        .map(|s| s.dispatcher.drums_snapshot())
}

/// Everything the panel draws: state, installed library, project outputs.
pub(crate) fn drums_panel_view(session: &SessionCell) -> Option<(DrumsSnapshot, DrumsView)> {
    let borrowed = session.borrow();
    let s = borrowed.as_ref()?;
    let snapshot = s.dispatcher.drums_snapshot();
    let library = s.dispatcher.drums_library();
    let devices = infra_cpal::list_output_device_descriptors().unwrap_or_default();
    let outputs = output_endpoints(&s.io_bindings.borrow(), &devices);
    let view = drums_view(&snapshot, &library, &outputs);
    Some((snapshot, view))
}

/// Send one drums command; `false` when there is no project or it was refused.
pub(crate) fn dispatch_drums(session: &SessionCell, command: DrumsCommand) -> bool {
    let borrowed = session.borrow();
    let Some(s) = borrowed.as_ref() else {
        return false;
    };
    if let Err(e) = s.dispatcher.dispatch(Command::Drums(command)) {
        log::warn!("[drums] dispatch failed: {e}");
        return false;
    }
    true
}

#[cfg(test)]
#[path = "drums_session_tests.rs"]
mod tests;
