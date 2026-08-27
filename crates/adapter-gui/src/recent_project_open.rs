//! Responsibility: opens the recent project the launcher row points at.
//!
//! Split out of `recent_projects_wiring` (#913). Switching the view is what a
//! window does; the open itself is a sequence whose ORDER is load-bearing —
//! stop the previous rig, wire the audio seam to the NEW session, put
//! `LoadProject` on the bus, then publish the rows. #903: the recorded loops
//! ride on `LoadProject` and need a store to land in, so the seam is attached
//! before the dispatch and after the old rig is gone, or they land in the
//! controller that is about to be dropped.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use application::command::{Command, ProjectCommand};
use domain::AudioDeviceDescriptor;
use infra_filesystem::AppConfig;
use slint::VecModel;

use crate::project_file_dialog_wiring::stop_the_previous_rig;
use crate::project_ops::{
    canonical_project_path, load_project_session, mark_recent_project_invalid,
    project_display_name, project_session_snapshot, project_title_for_path, recent_project_items,
    register_recent_project, resolve_project_config_path,
};
use crate::project_view::replace_project_chains;
use crate::runtime_lifecycle::RuntimeAttach;
use crate::state::ProjectSession;
use crate::{ProjectChainItem, RecentProjectItem};

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

/// What the window needs to finish showing the opened project.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct OpenedRecent {
    pub(crate) title: String,
    pub(crate) canonical_path: PathBuf,
}

/// The handles an open touches. Grouped because the sequence needs all of them
/// and threading nine parameters through reads worse than naming them once.
pub(crate) struct OpenRecentCtx<'a> {
    pub(crate) app_config: &'a Rc<RefCell<AppConfig>>,
    pub(crate) recent_projects: &'a Rc<VecModel<RecentProjectItem>>,
    pub(crate) project_session: &'a Rc<RefCell<Option<ProjectSession>>>,
    pub(crate) project_chains: &'a Rc<VecModel<ProjectChainItem>>,
    pub(crate) runtime_attach: &'a RuntimeAttach,
    pub(crate) saved_project_snapshot: &'a Rc<RefCell<Option<String>>>,
    pub(crate) input_chain_devices: &'a [AudioDeviceDescriptor],
    pub(crate) output_chain_devices: &'a [AudioDeviceDescriptor],
    /// The launcher's current search text, so the refreshed list keeps it.
    pub(crate) search: &'a str,
}

/// Open the recent entry at `index`.
///
/// The in-memory `AppConfig` is updated either way (a new recent entry, or the
/// invalid flag); PERSISTING it is the caller's, so this never writes the
/// machine's `config.yaml` (#701).
pub(crate) fn open_recent(
    ctx: &OpenRecentCtx<'_>,
    index: usize,
) -> Result<OpenedRecent, OpenRecentError> {
    let Some(recent) = ctx.app_config.borrow().recent_projects.get(index).cloned() else {
        return Err(OpenRecentError::NoSuchEntry);
    };
    if !recent.is_valid {
        return Err(OpenRecentError::AlreadyInvalid(recent.invalid_reason));
    }
    let path = PathBuf::from(&recent.project_path);
    let session = match load_project_session(&path, &resolve_project_config_path(&path)) {
        Ok(session) => session,
        Err(error) => {
            mark_recent_project_invalid(
                &mut ctx.app_config.borrow_mut(),
                &path,
                &error.to_string(),
            );
            // #436: the invalidation goes on the bus too when there is one —
            // the open failed, so there may be no session at all.
            if let Some(session) = ctx.project_session.borrow().as_ref() {
                let _ = session.dispatcher.dispatch(Command::Project(
                    ProjectCommand::MarkRecentProjectInvalid {
                        path: path.clone(),
                        reason: error.to_string(),
                    },
                ));
            }
            republish_recents(ctx);
            return Err(OpenRecentError::LoadFailed);
        }
    };

    let canonical_path = canonical_project_path(&path).unwrap_or_else(|_| path.clone());
    let title = project_title_for_path(Some(&canonical_path), &session.project.borrow());
    let display_name = project_display_name(&session.project.borrow());

    stop_the_previous_rig(ctx.project_session);
    // #903/#127: the audio seam is wired to the NEW session before anything can
    // dispatch against it, and after the previous rig is stopped.
    ctx.runtime_attach.to_session(&session);
    {
        let project = session.project.borrow().clone();
        if let Err(e) = session
            .dispatcher
            .dispatch(Command::Project(ProjectCommand::LoadProject {
                project,
                path: canonical_path.clone(),
            }))
        {
            log::warn!("[open-recent] Command::LoadProject failed: {e}");
        }
    }
    replace_project_chains(
        ctx.project_chains,
        &session.project.borrow(),
        ctx.input_chain_devices,
        ctx.output_chain_devices,
        &[],
    );
    // #808: the rows were built with an empty binding registry, so the DI
    // output select stayed empty until the chain was first enabled.
    crate::di_output_options::apply_di_outputs_to_rows(
        ctx.project_chains,
        &session.project.borrow(),
        &session.io_bindings.borrow(),
    );
    let snapshot = project_session_snapshot(&session).ok();
    *ctx.project_session.borrow_mut() = Some(session);
    *ctx.saved_project_snapshot.borrow_mut() = snapshot;
    register_recent_project(
        &mut ctx.app_config.borrow_mut(),
        &canonical_path,
        &display_name,
    );
    if let Some(session) = ctx.project_session.borrow().as_ref() {
        let _ =
            session
                .dispatcher
                .dispatch(Command::Project(ProjectCommand::RegisterRecentProject {
                    path: canonical_path.clone(),
                    name: display_name.clone(),
                }));
    }
    republish_recents(ctx);
    Ok(OpenedRecent {
        title,
        canonical_path,
    })
}

fn republish_recents(ctx: &OpenRecentCtx<'_>) {
    ctx.recent_projects.set_vec(recent_project_items(
        &ctx.app_config.borrow().recent_projects,
        ctx.search,
    ));
}

#[cfg(test)]
#[path = "recent_project_open_tests.rs"]
mod tests;
