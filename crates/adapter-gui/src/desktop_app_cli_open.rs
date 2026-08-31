//! Responsibility: opens the project named on the command line.
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

use slint::VecModel;

use crate::project_ops::set_project_dirty;
use crate::state::ProjectSession;
use crate::{AppWindow, ProjectChainItem, RecentProjectItem};

use domain::AudioDeviceDescriptor;
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
    let Some(cli_path) = cli_project_path else {
        return;
    };
    let Some(opened) = crate::cli_project_open::load_cli_project(
        cli_path,
        project_session,
        project_chains,
        &input_chain_devices.borrow(),
        &output_chain_devices.borrow(),
        saved_project_snapshot,
        app_config,
        recent_projects,
    ) else {
        return;
    };
    {
        // #693: the config write runs on the persist worker — the GUI thread
        // never waits on disk. #731: the config path is bound at dispatch time.
        let snapshot = app_config.borrow().clone();
        application::app_config_persist::persist_app_config_snapshot(snapshot);
    }
    crate::chain_rig_nav_wiring::refresh_from_session(window, project_session);
    set_project_dirty(window, project_dirty, false);
    window.set_project_title(opened.title.into());
    window.set_project_path_label(
        rust_i18n::t!(
            "status-project-path-prefix",
            path = opened.canonical_path.display()
        )
        .to_string()
        .into(),
    );
    window.set_show_project_launcher(false);
    window.set_show_project_setup(false);
    window.set_show_project_chains(true);
    window.set_skip_launcher(true);
}
