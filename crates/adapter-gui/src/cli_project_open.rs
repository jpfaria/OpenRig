//! Responsibility: loads the project named on the command line into this session.
//!
//! Split out of `desktop_app_cli_open` (#913). Switching the view to the chains
//! screen is what a window does; loading the file, publishing its chain rows,
//! recording it as recent and taking the clean snapshot is what has to happen
//! either way — and a failure has to leave the session untouched, because the
//! app still has to come up on the launcher with a bad path on the command line.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use application::command::{Command, ProjectCommand};
use domain::AudioDeviceDescriptor;
use infra_filesystem::AppConfig;
use slint::VecModel;

use crate::project_ops::{
    canonical_project_path, open_cli_project, project_display_name, project_session_snapshot,
    project_title_for_path, recent_project_items, register_recent_project,
};
use crate::project_view::replace_project_chains;
use crate::state::ProjectSession;
use crate::{ProjectChainItem, RecentProjectItem};

/// What the window needs in order to finish showing an opened CLI project.
pub(crate) struct CliOpened {
    /// The window title for the loaded project.
    pub(crate) title: String,
    /// The path the project was actually loaded from, canonicalised.
    pub(crate) canonical_path: PathBuf,
}

/// Load `cli_path` and install it as this session's project.
///
/// `None` ⇒ the file could not be opened; NOTHING was changed and the caller
/// stays on the launcher. A partially-applied open (rows replaced, session
/// still empty) would show a project that is not loaded.
///
/// The in-memory `app_config` gains the new recent entry; PERSISTING it is the
/// caller's, so this never writes the machine's `config.yaml` (#701).
#[allow(clippy::too_many_arguments)]
pub(crate) fn load_cli_project(
    cli_path: &PathBuf,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    project_chains: &Rc<VecModel<ProjectChainItem>>,
    input_chain_devices: &[AudioDeviceDescriptor],
    output_chain_devices: &[AudioDeviceDescriptor],
    saved_project_snapshot: &Rc<RefCell<Option<String>>>,
    app_config: &Rc<RefCell<AppConfig>>,
    recent_projects: &Rc<VecModel<RecentProjectItem>>,
) -> Option<CliOpened> {
    let session = match open_cli_project(cli_path) {
        Ok(session) => session,
        Err(e) => {
            log::error!("CLI project open failed, falling back to launcher: {e}");
            return None;
        }
    };
    let canonical_path = canonical_project_path(cli_path).unwrap_or_else(|_| cli_path.clone());
    let title = project_title_for_path(Some(&canonical_path), &session.project.borrow());
    let display_name = project_display_name(&session.project.borrow());
    replace_project_chains(
        project_chains,
        &session.project.borrow(),
        input_chain_devices,
        output_chain_devices,
        &[],
    );
    // #808: populate the DI output select from the real bindings now, or it
    // stays empty until the chain is first enabled.
    crate::di_output_options::apply_di_outputs_to_rows(
        project_chains,
        &session.project.borrow(),
        &session.io_bindings.borrow(),
    );
    let snapshot = project_session_snapshot(&session).ok();
    *project_session.borrow_mut() = Some(session);
    *saved_project_snapshot.borrow_mut() = snapshot;
    register_recent_project(&mut app_config.borrow_mut(), &canonical_path, &display_name);
    // #436 (sweep): registering the recent goes on the bus too, so a client
    // sees the startup open like any other.
    if let Some(session) = project_session.borrow().as_ref() {
        let _ =
            session
                .dispatcher
                .dispatch(Command::Project(ProjectCommand::RegisterRecentProject {
                    path: canonical_path.clone(),
                    name: display_name.clone(),
                }));
    }
    recent_projects.set_vec(recent_project_items(
        &app_config.borrow().recent_projects,
        "",
    ));
    log::info!("CLI: opened {:?}", canonical_path);
    Some(CliOpened {
        title,
        canonical_path,
    })
}

#[cfg(test)]
#[path = "cli_project_open_tests.rs"]
mod tests;
