//! Project / Metadata section wiring (#513). Name auto-saves on edit
//! (dispatches `UpdateProjectName`); the path is a read-only display
//! sourced from the active `ProjectSession`.
//!
//! Pattern mirrors `midi_devices::install` — takes the project session
//! Rc so it can no-op when no session is loaded.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, ProjectCommand};
use application::dispatcher::CommandDispatcher;

use crate::state::ProjectSession;
use crate::{AppWindow, ProjectSettingsWindow};

#[cfg(test)]
#[path = "project_meta_tests.rs"]
mod project_meta_tests;

pub(crate) fn sanitize_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn should_dispatch_rename(old: Option<&str>, new: Option<&str>) -> bool {
    old != new
}

/// Install the project-name edit callback on the AppWindow. Dispatches
/// `UpdateProjectName` only when the trimmed value differs from the last
/// one dispatched, so rapid edits do not flood the bus.
pub fn install(
    win: &AppWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    last_dispatched: Rc<RefCell<Option<String>>>,
) {
    let last_dispatched_for_edit = last_dispatched.clone();
    let project_session_for_edit = project_session.clone();
    win.on_edit_project_name(move |raw| {
        let new = sanitize_name(raw.as_str());
        let mut tracker = last_dispatched_for_edit.borrow_mut();
        if !should_dispatch_rename(tracker.as_deref(), new.as_deref()) {
            return;
        }
        *tracker = new.clone();
        let session = project_session_for_edit.borrow();
        let Some(session) = session.as_ref() else {
            return;
        };
        if let Err(e) = session
            .dispatcher
            .dispatch(Command::Project(ProjectCommand::UpdateProjectName {
                name: new.unwrap_or_default(),
            })) {
            log::warn!("[project_meta] Command::UpdateProjectName failed: {e}");
        }
    });
}

/// Mirror of `install` for the standalone `ProjectSettingsWindow`
/// (issue #513). Same Rc state as the main window so name edits made
/// in either surface dispatch a single `UpdateProjectName` and dedupe
/// through the shared `last_dispatched` tracker.
pub fn install_secondary(
    win: &ProjectSettingsWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    last_dispatched: Rc<RefCell<Option<String>>>,
) {
    let last_dispatched_for_edit = last_dispatched.clone();
    let project_session_for_edit = project_session.clone();
    win.on_edit_project_name(move |raw| {
        let new = sanitize_name(raw.as_str());
        let mut tracker = last_dispatched_for_edit.borrow_mut();
        if !should_dispatch_rename(tracker.as_deref(), new.as_deref()) {
            return;
        }
        *tracker = new.clone();
        let session = project_session_for_edit.borrow();
        let Some(session) = session.as_ref() else {
            return;
        };
        if let Err(e) = session
            .dispatcher
            .dispatch(Command::Project(ProjectCommand::UpdateProjectName {
                name: new.unwrap_or_default(),
            })) {
            log::warn!("[project_meta] Command::UpdateProjectName failed: {e}");
        }
    });
}
