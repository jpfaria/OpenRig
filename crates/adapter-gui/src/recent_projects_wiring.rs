//! Wiring for the launcher's "recent projects" callbacks on the main window.
//!
//! Owns the 3 callbacks driving the recent-projects list:
//!
//! - `on_filter_recent_projects` — refilters the list by the current search
//!   query and stores it on the window for later refresh round-trips.
//! - `on_open_recent_project` — loads the project at the recent index, swaps
//!   the runtime, replaces the chain rows, refreshes the recent list, and
//!   transitions the launcher into the chains view. Marks the entry invalid
//!   on load failure so the user can clean it up.
//! - `on_remove_recent_project` — drops an entry from `app_config` and
//!   re-renders the list.
//!
//! Stays out of `lib.rs` so launcher tweaks don't collide with other UI work.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use slint::{ComponentHandle, Timer, VecModel};

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use infra_filesystem::AppConfig;

use crate::audio_devices::ensure_devices_loaded;
use crate::helpers::{clear_status, set_status_error};
use crate::project_ops::{
    canonical_project_path, load_project_session, mark_recent_project_invalid,
    project_display_name, project_session_snapshot, project_title_for_path, recent_project_items,
    register_recent_project, resolve_project_config_path, set_project_dirty,
};
use crate::project_view::replace_project_chains;
use crate::state::ProjectSession;
use crate::stop_project_runtime;
use crate::{AppWindow, ProjectChainItem, RecentProjectItem};

pub(crate) struct RecentProjectsCtx {
    pub app_config: Rc<RefCell<AppConfig>>,
    pub recent_projects: Rc<VecModel<RecentProjectItem>>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub toast_timer: Rc<Timer>,
}

pub(crate) fn wire(window: &AppWindow, ctx: RecentProjectsCtx) {
    let RecentProjectsCtx {
        app_config,
        recent_projects,
        project_session,
        project_chains,
        project_runtime,
        saved_project_snapshot,
        project_dirty,
        input_chain_devices,
        output_chain_devices,
        toast_timer,
    } = ctx;

    {
        let weak_window = window.as_weak();
        let app_config = app_config.clone();
        let recent_projects = recent_projects.clone();
        window.on_filter_recent_projects(move |query| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            recent_projects.set_vec(recent_project_items(
                &app_config.borrow().recent_projects,
                query.as_str(),
            ));
            window.set_recent_project_search(query);
        });
    }
    {
        let weak_window = window.as_weak();
        let app_config = app_config.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let recent_projects = recent_projects.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_open_recent_project(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            ensure_devices_loaded(&input_chain_devices, &output_chain_devices);
            let Some(recent) = app_config
                .borrow()
                .recent_projects
                .get(index as usize)
                .cloned()
            else {
                set_status_error(
                    &window,
                    &toast_timer,
                    &rust_i18n::t!("error-invalid-recent-project"),
                );
                return;
            };
            if !recent.is_valid {
                set_status_error(
                    &window,
                    &toast_timer,
                    &recent.invalid_reason.unwrap_or_else(|| {
                        rust_i18n::t!("error-invalid-recent-project").to_string()
                    }),
                );
                return;
            }
            let path = PathBuf::from(&recent.project_path);
            match load_project_session(&path, &resolve_project_config_path(&path)) {
                Ok(session) => {
                    let canonical_path = canonical_project_path(&path).unwrap_or(path.clone());
                    // #436 E: abrir recente é negócio → Command::LoadProject
                    // no dispatcher da sessão (MCP/MIDI, observável via
                    // Event::ProjectLoaded). Load+swap é adapter-side
                    // (precedente SaveProject).
                    {
                        let project = session.project.borrow().clone();
                        if let Err(e) = session.dispatcher.dispatch(Command::LoadProject {
                            project,
                            path: canonical_path.clone(),
                        }) {
                            log::warn!("[open-recent] Command::LoadProject falhou: {e}");
                        }
                    }
                    let title =
                        project_title_for_path(Some(&canonical_path), &*session.project.borrow());
                    let display_name = project_display_name(&*session.project.borrow());
                    stop_project_runtime(&project_runtime);
                    replace_project_chains(
                        &project_chains,
                        &*session.project.borrow(),
                        &input_chain_devices.borrow(),
                        &output_chain_devices.borrow(),
                    );
                    let snapshot = project_session_snapshot(&session).ok();
                    *project_session.borrow_mut() = Some(session);
                    crate::chain_rig_nav_wiring::refresh_from_session(&window, &project_session);
                    *saved_project_snapshot.borrow_mut() = snapshot;
                    register_recent_project(
                        &mut app_config.borrow_mut(),
                        &canonical_path,
                        &display_name,
                    );
                    // #436 (sweep): registrar recente via Command.
                    if let Some(s) = project_session.borrow().as_ref() {
                        let _ = s.dispatcher.dispatch(Command::RegisterRecentProject {
                            path: canonical_path.clone(),
                            name: display_name.clone(),
                        });
                    }
                    {
                        // #693: config write runs on the persist worker — the
                        // GUI thread never waits on disk.
                        let snapshot = app_config.borrow().clone();
                        // #731: bind the config path at dispatch time.
                        application::app_config_persist::persist_app_config_snapshot(snapshot);
                    }
                    recent_projects.set_vec(recent_project_items(
                        &app_config.borrow().recent_projects,
                        window.get_recent_project_search().as_str(),
                    ));
                    set_project_dirty(&window, &project_dirty, false);
                    clear_status(&window, &toast_timer);
                    window.set_project_title(title.into());
                    window.set_project_name_draft(
                        project_session
                            .borrow()
                            .as_ref()
                            .and_then(|session| session.project.borrow().name.clone())
                            .unwrap_or_default()
                            .into(),
                    );
                    window.set_project_path_label(
                        rust_i18n::t!(
                            "status-project-path-prefix",
                            path = canonical_path.display()
                        )
                        .to_string()
                        .into(),
                    );
                    window.set_show_project_launcher(false);
                    window.set_show_project_chains(true);
                    window.set_show_chain_editor(false);
                    window.set_show_settings(false);
                }
                Err(error) => {
                    let reason = error.to_string();
                    mark_recent_project_invalid(&mut app_config.borrow_mut(), &path, &reason);
                    // #436 (sweep): invalidar recente via Command (quando
                    // há sessão; open falhou, pode não haver). Persist
                    // abaixo é adapter-side (precedente SaveProject).
                    if let Some(s) = project_session.borrow().as_ref() {
                        let _ = s.dispatcher.dispatch(Command::MarkRecentProjectInvalid {
                            path: path.clone(),
                            reason,
                        });
                    }
                    {
                        // #693: config write runs on the persist worker — the
                        // GUI thread never waits on disk.
                        let snapshot = app_config.borrow().clone();
                        // #731: bind the config path at dispatch time.
                        application::app_config_persist::persist_app_config_snapshot(snapshot);
                    }
                    recent_projects.set_vec(recent_project_items(
                        &app_config.borrow().recent_projects,
                        window.get_recent_project_search().as_str(),
                    ));
                    set_status_error(
                        &window,
                        &toast_timer,
                        &rust_i18n::t!("error-invalid-recent-project-detail"),
                    );
                }
            }
        });
    }
    // Issue #360: remove-recent now opens an in-window overlay before
    // touching app_config. The dispatch + filesystem persist live in
    // confirm-delete-recent-project below; cancel just hides the modal.
    let pending_remove_recent: Rc<RefCell<Option<usize>>> = Rc::new(RefCell::new(None));
    {
        let weak_window = window.as_weak();
        let app_config = app_config.clone();
        let pending = pending_remove_recent.clone();
        window.on_remove_recent_project(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let config = app_config.borrow();
            let idx = index as usize;
            let Some(entry) = config.recent_projects.get(idx) else {
                return;
            };
            let display_name = if entry.project_name.is_empty() {
                std::path::Path::new(&entry.project_path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| entry.project_path.clone())
            } else {
                entry.project_name.clone()
            };
            *pending.borrow_mut() = Some(idx);
            window.set_confirm_delete_recent_project_name(display_name.into());
            window.set_show_confirm_delete_recent_project(true);
        });
    }
    {
        let weak_window = window.as_weak();
        let pending = pending_remove_recent.clone();
        window.on_cancel_delete_recent_project(move || {
            *pending.borrow_mut() = None;
            if let Some(window) = weak_window.upgrade() {
                window.set_show_confirm_delete_recent_project(false);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let app_config = app_config.clone();
        let recent_projects = recent_projects.clone();
        let project_session = project_session.clone();
        let pending = pending_remove_recent.clone();
        window.on_confirm_delete_recent_project(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            window.set_show_confirm_delete_recent_project(false);
            let Some(index) = pending.borrow_mut().take() else {
                return;
            };
            let mut config = app_config.borrow_mut();
            if index < config.recent_projects.len() {
                // #436 F: remover recente é negócio → Command no
                // dispatcher compartilhado (MCP/MIDI, observável via
                // Event::RecentProjectRemoved) quando há sessão. A
                // mutação/persistência do app-config + render abaixo é
                // adapter-side (precedente SaveProject).
                if let Some(session) = project_session.borrow().as_ref() {
                    if let Err(e) = session
                        .dispatcher
                        .dispatch(Command::RemoveRecentProject { index })
                    {
                        log::warn!("[recent] Command::RemoveRecentProject falhou: {e}");
                    }
                }
                config.recent_projects.remove(index);
                {
                    // #693: write on the persist worker.
                    let snapshot = config.clone();
                    // #731: bind the config path at dispatch time.
                    application::app_config_persist::persist_app_config_snapshot(snapshot);
                }
                recent_projects.set_vec(recent_project_items(
                    &config.recent_projects,
                    window.get_recent_project_search().as_str(),
                ));
            }
        });
    }
}
