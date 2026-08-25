//! Responsibility: wires the project-level callbacks of the main window.
//!
//! Opening, saving and switching a project: the file dialogs, the recent list,
//! the project settings window and the chain-preset picker. Everything here
//! acts on the SESSION as a whole; per-chain and per-block callbacks live in
//! `desktop_app_chain_wiring` / `desktop_app_block_wiring`.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Timer, VecModel};

use crate::state::{AudioSettingsMode, ProjectPaths, ProjectSession};
use crate::{AppWindow, ProjectSettingsWindow};

pub(crate) struct ProjectWiringDeps {
    pub project_paths: ProjectPaths,
    pub app_config: Rc<RefCell<infra_filesystem::AppConfig>>,
    pub recent_projects: Rc<VecModel<crate::RecentProjectItem>>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<crate::ProjectChainItem>>,
    pub project_devices: Rc<VecModel<crate::DeviceSelectionItem>>,
    pub runtime_attach: crate::runtime_lifecycle::RuntimeAttach,
    pub chain_input_device_options: Rc<VecModel<slint::SharedString>>,
    pub chain_output_device_options: Rc<VecModel<slint::SharedString>>,
    pub input_chain_devices: Rc<RefCell<Vec<domain::AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<domain::AudioDeviceDescriptor>>>,
    pub audio_settings_mode: Rc<RefCell<AudioSettingsMode>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub preset_file_list: Rc<RefCell<Vec<std::path::PathBuf>>>,
    pub toast_timer: Rc<Timer>,
    pub auto_save: bool,
    pub fullscreen: bool,
}

pub(crate) fn wire(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    deps: ProjectWiringDeps,
) {
    let ProjectWiringDeps {
        project_paths,
        app_config,
        recent_projects,
        project_session,
        project_chains,
        project_devices,
        runtime_attach,
        chain_input_device_options,
        chain_output_device_options,
        input_chain_devices,
        output_chain_devices,
        audio_settings_mode,
        saved_project_snapshot,
        project_dirty,
        preset_file_list,
        toast_timer,
        auto_save,
        fullscreen,
    } = deps;
    // --- Project file dialog callbacks (extracted to project_file_dialog_wiring) ---
    crate::project_file_dialog_wiring::wire(
        window,
        crate::project_file_dialog_wiring::ProjectFileDialogCtx {
            project_paths: project_paths.clone(),
            app_config: app_config.clone(),
            recent_projects: recent_projects.clone(),
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            runtime_attach: runtime_attach.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            toast_timer: toast_timer.clone(),
        },
    );
    // --- Recent projects callbacks (extracted to recent_projects_wiring) ---
    crate::recent_projects_wiring::wire(
        window,
        crate::recent_projects_wiring::RecentProjectsCtx {
            app_config: app_config.clone(),
            recent_projects: recent_projects.clone(),
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            runtime_attach: runtime_attach.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            toast_timer: toast_timer.clone(),
        },
    );
    // --- Project settings callbacks (extracted to project_settings_wiring) ---
    crate::project_settings_wiring::wire(
        window,
        project_settings_window,
        crate::project_settings_wiring::ProjectSettingsCtx {
            project_session: project_session.clone(),
            project_devices: project_devices.clone(),
            chain_input_device_options: chain_input_device_options.clone(),
            chain_output_device_options: chain_output_device_options.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            audio_settings_mode: audio_settings_mode.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            toast_timer: toast_timer.clone(),
            auto_save,
            fullscreen,
        },
    );
    // --- Chain preset callbacks (extracted to chain_preset_wiring) ---
    crate::chain_preset_wiring::wire(
        window,
        crate::chain_preset_wiring::ChainPresetCtx {
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            toast_timer: toast_timer.clone(),
            preset_file_list: preset_file_list.clone(),
            auto_save,
        },
    );
}
