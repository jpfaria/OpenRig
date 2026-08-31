//! Responsibility: records the project name the user is typing.
//!
//! Split out of `project_settings_wiring` (#913), which dispatched this from
//! two callbacks — the main window's field and the settings window's — with a
//! copy each. Mirroring the text onto both surfaces is screen work; putting the
//! rename on the bus is not, or a client never learns the project was renamed.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, ProjectCommand};

use crate::state::ProjectSession;

/// Dispatch the rename. Returns whether it reached a dispatcher — the settings
/// screen is reachable with no project open, and typing there renames nothing.
pub(crate) fn record_project_name(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    name: &str,
) -> bool {
    let borrowed = project_session.borrow();
    let Some(session) = borrowed.as_ref() else {
        return false;
    };
    if let Err(e) =
        session
            .dispatcher
            .dispatch(Command::Project(ProjectCommand::UpdateProjectName {
                name: name.to_string(),
            }))
    {
        log::warn!("[project-name] rename failed: {e}");
        return false;
    }
    true
}

#[cfg(test)]
#[path = "project_name_edit_tests.rs"]
mod tests;
