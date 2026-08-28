//! Responsibility: opens the recent project the launcher row points at.
//!
//! Split out of `recent_projects_wiring` (#913). The open itself is
//! `project_open` — shared with the file dialog; what belongs to the RECENT
//! LIST is the part around it: resolving the row, refusing an entry already
//! known bad, and flagging one that fails now so the user can clean it up
//! instead of clicking a dead row forever.

use std::path::PathBuf;

use application::command::{Command, ProjectCommand};

use crate::project_open::{open_project_at, republish_recents, OpenProjectCtx, OpenedProject};
use crate::project_ops::mark_recent_project_invalid;

/// Why a recent row could not be opened. Each carries what the toast says.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum OpenRecentError {
    /// The row index is not in the recent list any more.
    NoSuchEntry,
    /// The entry is already flagged invalid; carries the recorded reason.
    AlreadyInvalid(Option<String>),
    /// The load failed now; the entry has been flagged and the list refreshed.
    LoadFailed,
}

/// Open the recent entry at `index`.
pub(crate) fn open_recent(
    ctx: &OpenProjectCtx<'_>,
    index: usize,
) -> Result<OpenedProject, OpenRecentError> {
    let Some(recent) = ctx.app_config.borrow().recent_projects.get(index).cloned() else {
        return Err(OpenRecentError::NoSuchEntry);
    };
    if !recent.is_valid {
        return Err(OpenRecentError::AlreadyInvalid(recent.invalid_reason));
    }
    let path = PathBuf::from(&recent.project_path);
    match open_project_at(ctx, &path) {
        Ok(opened) => Ok(opened),
        Err(reason) => {
            mark_recent_project_invalid(&mut ctx.app_config.borrow_mut(), &path, &reason);
            // #436: the invalidation goes on the bus too when there is one —
            // the open failed, so there may be no session at all.
            if let Some(session) = ctx.project_session.borrow().as_ref() {
                let _ = session.dispatcher.dispatch(Command::Project(
                    ProjectCommand::MarkRecentProjectInvalid { path, reason },
                ));
            }
            republish_recents(ctx);
            Err(OpenRecentError::LoadFailed)
        }
    }
}

#[cfg(test)]
#[path = "recent_project_open_tests.rs"]
mod tests;
