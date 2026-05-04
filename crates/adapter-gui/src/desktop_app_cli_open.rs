//! Auto-opens a project file passed on the command line.
//!
//! When `cli_project_path` is `Some`, loads the YAML, registers it in the
//! recent-projects list, replaces the chain rows model, and skips the
//! launcher screen straight to the chains view. Failures fall back to the
//! launcher silently — the user still gets a usable app even when the path
//! is bad. Pure UI/state plumbing; no audio side effects.
//!
//! Called once from `run_desktop_app` after the main window and its initial
//! state are constructed.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use infra_filesystem::FilesystemStorage;
use slint::VecModel;

use crate::project_ops::{
    canonical_project_path, open_cli_project, project_display_name, project_session_snapshot,
    project_title_for_path, recent_project_items, register_recent_project, set_project_dirty,
};
use crate::project_view::replace_project_chains;
use crate::state::ProjectSession;
use crate::{AppWindow, ProjectChainItem, RecentProjectItem};

use infra_cpal::AudioDeviceDescriptor;
use infra_filesystem::AppConfig;

#[allow(clippy::too_many_arguments)]
pub(crate) fn try_auto_open(
    cli_project_path: Option<&PathBuf>,
    window: &AppWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    project_chains: &Rc<VecModel<ProjectChainItem>>,
    input_chain_devices: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_chain_devices: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    saved_project_snapshot: &Rc<RefCell<Option<String>>>,
    project_dirty: &Rc<RefCell<bool>>,
    app_config: &Rc<RefCell<AppConfig>>,
    recent_projects: &Rc<VecModel<RecentProjectItem>>,
) {
    let Some(cli_path) = cli_project_path else { return };
    match open_cli_project(cli_path) {
        Ok(session) => {
            let canonical_path = canonical_project_path(cli_path).unwrap_or(cli_path.clone());
            let title = project_title_for_path(Some(&canonical_path), &session.project);
            let display_name = project_display_name(&session.project);
            replace_project_chains(
                project_chains,
                &session.project,
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
            );
            let snapshot = project_session_snapshot(&session).ok();
            *project_session.borrow_mut() = Some(session);
            *saved_project_snapshot.borrow_mut() = snapshot;
            register_recent_project(&mut app_config.borrow_mut(), &canonical_path, &display_name);
            let _ = FilesystemStorage::save_app_config(&app_config.borrow());
            recent_projects.set_vec(recent_project_items(&app_config.borrow().recent_projects, ""));
            set_project_dirty(window, project_dirty, false);
            window.set_project_title(title.into());
            window.set_project_path_label(
                rust_i18n::t!("status-project-path-prefix", path = canonical_path.display()).to_string().into(),
            );
            window.set_show_project_launcher(false);
            window.set_show_project_setup(false);
            window.set_show_project_chains(true);
            window.set_skip_launcher(true);
            log::info!("CLI: opened {:?}", canonical_path);
        }
        Err(e) => {
            log::error!("CLI project open failed, falling back to launcher: {e}");
        }
    }
}
