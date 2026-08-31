//! Responsibility: drops one entry from the launcher's recent list.
//!
//! Split out of `recent_projects_wiring` (#913). Hiding the confirmation is
//! screen work; removing the entry is not. The removal goes on the bus so a
//! client sees `Event::RecentProjectRemoved` (#436 F), and the in-memory
//! `AppConfig` is mutated so the re-render and the next wholesale save agree
//! with what the user just confirmed.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, ProjectCommand};
use infra_filesystem::AppConfig;
use slint::VecModel;

use crate::project_ops::recent_project_items;
use crate::state::ProjectSession;
use crate::RecentProjectItem;

/// Remove the recent entry at `index` and republish the list.
///
/// Returns whether anything was removed. A confirmation can outlive the list it
/// was raised on — another transport may have removed the entry first — and a
/// stale index must drop nothing rather than take out its neighbour.
pub(crate) fn remove_recent(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    app_config: &Rc<RefCell<AppConfig>>,
    recent_projects: &Rc<VecModel<RecentProjectItem>>,
    index: usize,
    search: &str,
) -> bool {
    let mut config = app_config.borrow_mut();
    if index >= config.recent_projects.len() {
        return false;
    }
    if let Some(session) = project_session.borrow().as_ref() {
        if let Err(e) =
            session
                .dispatcher
                .dispatch(Command::Project(ProjectCommand::RemoveRecentProject {
                    index,
                }))
        {
            log::warn!("[recent] Command::RemoveRecentProject failed: {e}");
        }
    }
    config.recent_projects.remove(index);
    recent_projects.set_vec(recent_project_items(&config.recent_projects, search));
    true
}

#[cfg(test)]
#[path = "recent_project_remove_tests.rs"]
mod tests;
