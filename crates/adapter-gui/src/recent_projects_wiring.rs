//! Responsibility: wires the launcher's recent projects list.
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
use std::rc::Rc;

use slint::{ComponentHandle, Global, Timer, VecModel};

use application::command::{Command, ProjectCommand};
use domain::AudioDeviceDescriptor;
use infra_filesystem::AppConfig;

use crate::audio_devices::ensure_devices_loaded;
use crate::helpers::{clear_status, set_status_error};
use crate::project_ops::{recent_project_items, set_project_dirty};
use crate::runtime_lifecycle::RuntimeAttach;
use crate::state::ProjectSession;
use crate::{AppWindow, ProjectChainItem, RecentProjectItem};

pub(crate) struct RecentProjectsCtx {
    pub app_config: Rc<RefCell<AppConfig>>,
    pub recent_projects: Rc<VecModel<RecentProjectItem>>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    /// #127: the capability to hand a freshly built session's dispatcher this
    /// frontend's audio runtime. Not the runtime itself — opening a recent
    /// project wires the seam up, it does not reach through it.
    pub runtime_attach: RuntimeAttach,
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
        runtime_attach,
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
        let runtime_attach = runtime_attach.clone();
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
            let result = crate::recent_project_open::open_recent(
                &crate::project_open::OpenProjectCtx {
                    app_config: &app_config,
                    recent_projects: &recent_projects,
                    project_session: &project_session,
                    project_chains: &project_chains,
                    runtime_attach: &runtime_attach,
                    saved_project_snapshot: &saved_project_snapshot,
                    input_chain_devices: &input_chain_devices.borrow(),
                    output_chain_devices: &output_chain_devices.borrow(),
                    search: window.get_recent_project_search().as_str(),
                },
                index as usize,
            );
            // #693/#731: the config write runs on the persist worker (the GUI
            // thread never waits on disk) and the path is bound at dispatch
            // time. Either outcome above changed the in-memory snapshot.
            {
                let snapshot = app_config.borrow().clone();
                application::app_config_persist::persist_app_config_snapshot(snapshot);
            }
            let opened = match result {
                Ok(opened) => opened,
                Err(crate::recent_project_open::OpenRecentError::AlreadyInvalid(reason)) => {
                    set_status_error(
                        &window,
                        &toast_timer,
                        &reason.unwrap_or_else(|| {
                            rust_i18n::t!("error-invalid-recent-project").to_string()
                        }),
                    );
                    return;
                }
                Err(crate::recent_project_open::OpenRecentError::LoadFailed) => {
                    set_status_error(
                        &window,
                        &toast_timer,
                        &rust_i18n::t!("error-invalid-recent-project-detail"),
                    );
                    return;
                }
                Err(crate::recent_project_open::OpenRecentError::NoSuchEntry) => {
                    set_status_error(
                        &window,
                        &toast_timer,
                        &rust_i18n::t!("error-invalid-recent-project"),
                    );
                    return;
                }
            };
            crate::chain_rig_nav_wiring::refresh_from_session(&window, &project_session);
            set_project_dirty(&window, &project_dirty, false);
            clear_status(&window, &toast_timer);
            window.set_project_title(opened.title.into());
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
                    path = opened.canonical_path.display()
                )
                .to_string()
                .into(),
            );
            window.set_show_project_launcher(false);
            window.set_show_project_chains(true);
            window.set_show_chain_editor(false);
            window.set_show_settings(false);
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
            let display_name = crate::recent_project_label::confirm_removal_label(entry);
            *pending.borrow_mut() = Some(idx);
            crate::OverlayBridge::get(&window)
                .set_confirm_delete_recent_project_name(display_name.into());
            crate::OverlayBridge::get(&window).set_show_confirm_delete_recent_project(true);
        });
    }
    {
        let weak_window = window.as_weak();
        let pending = pending_remove_recent.clone();
        crate::OverlayBridge::get(window).on_cancel_delete_recent_project(move || {
            *pending.borrow_mut() = None;
            if let Some(window) = weak_window.upgrade() {
                crate::OverlayBridge::get(&window).set_show_confirm_delete_recent_project(false);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let app_config = app_config.clone();
        let recent_projects = recent_projects.clone();
        let project_session = project_session.clone();
        let pending = pending_remove_recent.clone();
        crate::OverlayBridge::get(window).on_confirm_delete_recent_project(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            crate::OverlayBridge::get(&window).set_show_confirm_delete_recent_project(false);
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
                    if let Err(e) = session.dispatcher.dispatch(Command::Project(
                        ProjectCommand::RemoveRecentProject { index },
                    )) {
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
