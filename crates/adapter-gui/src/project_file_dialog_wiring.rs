//! Wiring for the launcher's project-file callbacks on the main window.
//!
//! Owns the 5 callbacks that drive the project-file dialog flow:
//!
//! - `on_open_project_file`    — open-file dialog → load → swap runtime → show chains
//! - `on_create_project_file`  — clear name draft and route to the new-project setup
//! - `on_confirm_new_project`  — validate name, build session, route to chains
//! - `on_cancel_new_project`   — back to launcher view
//! - `on_save_project`         — save to known path or save-as dialog, then refresh
//!                                recent list and toast on error
//!
//! Stays out of `lib.rs` so launcher tweaks don't collide with other UI work.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use rfd::FileDialog;
use slint::{ComponentHandle, Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use infra_filesystem::{AppConfig, FilesystemStorage};

use crate::audio_devices::ensure_devices_loaded;
use crate::helpers::{clear_status, set_status_error};
use crate::project_ops::{
    canonical_project_path, create_new_project_session, load_project_session, project_display_name,
    project_session_snapshot, project_title_for_path, recent_project_items,
    register_recent_project, resolve_project_config_path, save_project_session, set_project_dirty,
};
use crate::project_view::replace_project_chains;
use crate::state::{ProjectPaths, ProjectSession};
use crate::stop_project_runtime;
use crate::{AppWindow, ProjectChainItem, RecentProjectItem};

pub(crate) struct ProjectFileDialogCtx {
    pub project_paths: ProjectPaths,
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

pub(crate) fn wire(window: &AppWindow, ctx: ProjectFileDialogCtx) {
    let ProjectFileDialogCtx {
        project_paths,
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
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let recent_projects = recent_projects.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_open_project_file(move || {
            log::info!("on_open_project_file triggered");
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            ensure_devices_loaded(&input_chain_devices, &output_chain_devices);
            let Some(path) = FileDialog::new()
                .add_filter("OpenRig Project", &["yaml", "yml"])
                .set_title("Abrir projeto")
                .pick_file()
            else {
                return;
            };
            log::info!("opening project file: {:?}", path);
            match load_project_session(&path, &resolve_project_config_path(&path)) {
                Ok(session) => {
                    let canonical_path = canonical_project_path(&path).unwrap_or(path.clone());
                    let title = project_title_for_path(Some(&canonical_path), &session.project);
                    let display_name = project_display_name(&session.project);
                    stop_project_runtime(&project_runtime);
                    replace_project_chains(
                        &project_chains,
                        &session.project,
                        &input_chain_devices.borrow(),
                        &output_chain_devices.borrow(),
                    );
                    let snapshot = project_session_snapshot(&session).ok();
                    *project_session.borrow_mut() = Some(session);
                    *saved_project_snapshot.borrow_mut() = snapshot;
                    register_recent_project(
                        &mut app_config.borrow_mut(),
                        &canonical_path,
                        &display_name,
                    );
                    let _ = FilesystemStorage::save_app_config(&app_config.borrow());
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
                            .and_then(|session| session.project.name.clone())
                            .unwrap_or_default()
                            .into(),
                    );
                    window.set_project_path_label(
                        format!("Projeto: {}", canonical_path.display()).into(),
                    );
                    window.set_show_project_launcher(false);
                    window.set_show_project_setup(false);
                    window.set_show_project_chains(true);
                    window.set_show_chain_editor(false);
                    window.set_show_project_settings(false);
                }
                Err(error) => {
                    set_status_error(&window, &toast_timer, &error.to_string());
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let toast_timer = toast_timer.clone();
        window.on_create_project_file(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            clear_status(&window, &toast_timer);
            window.set_project_name_draft("".into());
            window.set_show_project_launcher(false);
            window.set_show_project_setup(true);
            window.set_show_project_chains(false);
            window.set_show_chain_editor(false);
            window.set_show_project_settings(false);
        });
    }
    {
        let weak_window = window.as_weak();
        let project_paths = project_paths.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_confirm_new_project(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let name = window.get_project_name_draft().trim().to_string();
            if name.is_empty() {
                set_status_error(&window, &toast_timer, &rust_i18n::t!("O nome do projeto é obrigatório."));
                return;
            }
            ensure_devices_loaded(&input_chain_devices, &output_chain_devices);
            stop_project_runtime(&project_runtime);
            let mut session = create_new_project_session(&project_paths.default_config_path);
            session.project.name = Some(name.clone());
            replace_project_chains(
                &project_chains,
                &session.project,
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
            );
            *project_session.borrow_mut() = Some(session);
            *saved_project_snapshot.borrow_mut() = None;
            clear_status(&window, &toast_timer);
            set_project_dirty(&window, &project_dirty, true);
            window.set_project_title(name.into());
            window.set_project_path_label("Projeto em memória".into());
            window.set_show_project_setup(false);
            window.set_show_project_launcher(false);
            window.set_show_project_chains(true);
            window.set_show_chain_editor(false);
            window.set_show_project_settings(false);
        });
    }
    {
        let weak_window = window.as_weak();
        let toast_timer = toast_timer.clone();
        window.on_cancel_new_project(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            clear_status(&window, &toast_timer);
            window.set_show_project_setup(false);
            window.set_show_project_launcher(true);
        });
    }
    {
        let weak_window = window.as_weak();
        let app_config = app_config.clone();
        let project_session = project_session.clone();
        let recent_projects = recent_projects.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let toast_timer = toast_timer.clone();
        window.on_save_project(move || {
            log::info!("on_save_project triggered");
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                set_status_error(&window, &toast_timer, &rust_i18n::t!("Nenhum projeto carregado."));
                return;
            };
            let project_path = if let Some(path) = session.project_path.clone() {
                path
            } else {
                let Some(path) = FileDialog::new()
                    .add_filter("OpenRig Project", &["yaml", "yml"])
                    .set_title("Salvar projeto")
                    .set_file_name("project.yaml")
                    .save_file()
                else {
                    return;
                };
                session.project_path = Some(path.clone());
                session.config_path = Some(resolve_project_config_path(&path));
                session.presets_path = path
                    .parent()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("presets");
                path
            };
            match save_project_session(session, &project_path) {
                Ok(()) => {
                    let canonical_path =
                        canonical_project_path(&project_path).unwrap_or(project_path.clone());
                    register_recent_project(
                        &mut app_config.borrow_mut(),
                        &canonical_path,
                        &project_display_name(&session.project),
                    );
                    let _ = FilesystemStorage::save_app_config(&app_config.borrow());
                    recent_projects.set_vec(recent_project_items(
                        &app_config.borrow().recent_projects,
                        window.get_recent_project_search().as_str(),
                    ));
                    window.set_project_title(
                        project_title_for_path(Some(&canonical_path), &session.project).into(),
                    );
                    window.set_project_name_draft(
                        session.project.name.clone().unwrap_or_default().into(),
                    );
                    window.set_project_path_label(
                        format!("Projeto: {}", project_path.display()).into(),
                    );
                    *saved_project_snapshot.borrow_mut() = project_session_snapshot(session).ok();
                    set_project_dirty(&window, &project_dirty, false);
                    clear_status(&window, &toast_timer);
                }
                Err(error) => {
                    set_status_error(&window, &toast_timer, &error.to_string());
                }
            }
        });
    }
}
