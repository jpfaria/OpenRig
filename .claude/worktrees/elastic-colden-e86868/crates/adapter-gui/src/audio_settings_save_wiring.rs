//! Wiring for `on_save_audio_settings` on the main window and the standalone
//! project-settings window.
//!
//! Two callbacks, both branching on `AudioSettingsMode` (Gui = first-run setup,
//! Project = per-project device config). Shared steps: pull selected device
//! rows, persist gui-settings.yaml, write into the project session, apply
//! settings to hardware (`infra_cpal::apply_device_settings`), and resync the
//! audio runtime — which on Linux/JACK restarts jackd if sample rate or buffer
//! size changed.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Timer, VecModel};

use domain::ids::DeviceId;
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use infra_filesystem::{FilesystemStorage, GuiAudioSettings};
use project::device::DeviceSettings;

use crate::audio_devices::selected_device_settings;
use crate::helpers::{clear_status, set_status_error, set_status_warning};
use crate::project_ops::{
    build_device_settings_from_gui, project_title_for_path, sync_project_dirty,
};
use crate::project_view::replace_project_chains;
use crate::state::{AudioSettingsMode, ProjectSession};
use crate::sync_project_runtime;
use crate::{AppWindow, DeviceSelectionItem, ProjectChainItem, ProjectSettingsWindow};

/// Read the persisted `language` field so audio-device saves don't clobber it.
/// Returns None when settings file is absent or has no language override.
fn current_language() -> Option<String> {
    FilesystemStorage::load_gui_audio_settings()
        .ok()
        .flatten()
        .and_then(|s| s.language)
}

pub(crate) struct AudioSettingsSaveCtx {
    pub input_devices: Rc<VecModel<DeviceSelectionItem>>,
    pub output_devices: Rc<VecModel<DeviceSelectionItem>>,
    pub project_devices: Rc<VecModel<DeviceSelectionItem>>,
    pub audio_settings_mode: Rc<RefCell<AudioSettingsMode>>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub toast_timer: Rc<Timer>,
    pub auto_save: bool,
}

fn project_device_settings_from_rows(
    rows: Vec<infra_filesystem::GuiAudioDeviceSettings>,
) -> Vec<DeviceSettings> {
    rows.into_iter()
        .map(|device| DeviceSettings {
            device_id: DeviceId(device.device_id),
            sample_rate: device.sample_rate,
            buffer_size_frames: device.buffer_size_frames,
            bit_depth: device.bit_depth,
            #[cfg(target_os = "linux")]
            realtime: device.realtime,
            #[cfg(target_os = "linux")]
            rt_priority: device.rt_priority,
            #[cfg(target_os = "linux")]
            nperiods: device.nperiods,
        })
        .collect()
}

pub(crate) fn wire(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    ctx: AudioSettingsSaveCtx,
) {
    let AudioSettingsSaveCtx {
        input_devices,
        output_devices,
        project_devices,
        audio_settings_mode,
        project_session,
        project_chains,
        project_runtime,
        saved_project_snapshot,
        project_dirty,
        input_chain_devices,
        output_chain_devices,
        toast_timer,
        auto_save,
    } = ctx;

    {
        let weak_window = window.as_weak();
        let input_devices = input_devices.clone();
        let output_devices = output_devices.clone();
        let project_devices = project_devices.clone();
        let audio_settings_mode = audio_settings_mode.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_save_audio_settings(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            match *audio_settings_mode.borrow() {
                AudioSettingsMode::Gui => {
                    let input_devices = match selected_device_settings(&input_devices, "input") {
                        Ok(devices) => devices,
                        Err(error) => {
                            set_status_error(&window, &toast_timer, &error.to_string());
                            return;
                        }
                    };
                    let output_devices = match selected_device_settings(&output_devices, "output") {
                        Ok(devices) => devices,
                        Err(error) => {
                            set_status_error(&window, &toast_timer, &error.to_string());
                            return;
                        }
                    };
                    let settings = GuiAudioSettings {
                        input_devices,
                        output_devices,
                        language: current_language(),
                    };
                    if !settings.is_complete() {
                        set_status_warning(
                            &window,
                            &toast_timer,
                            "Selecione pelo menos um input e um output antes de continuar.",
                        );
                        return;
                    }
                    match FilesystemStorage::save_gui_audio_settings(&settings) {
                        Ok(()) => {
                            // Update in-memory device settings and resync the
                            // audio runtime so changes take effect immediately.
                            // On Linux/JACK this will restart jackd if sample
                            // rate or buffer size changed.
                            if let Some(session) = project_session.borrow_mut().as_mut() {
                                session.project.device_settings = build_device_settings_from_gui(
                                    &settings.input_devices,
                                    &settings.output_devices,
                                );
                                if let Err(e) = infra_cpal::apply_device_settings(
                                    &session.project.device_settings,
                                ) {
                                    log::warn!("apply_device_settings failed: {e}");
                                }
                                if let Err(e) = sync_project_runtime(&project_runtime, session) {
                                    set_status_error(&window, &toast_timer, &e.to_string());
                                    return;
                                }
                            }
                            clear_status(&window, &toast_timer);
                            window.set_show_audio_settings(false);
                        }
                        Err(error) => set_status_error(&window, &toast_timer, &error.to_string()),
                    }
                }
                AudioSettingsMode::Project => {
                    let project_device_settings =
                        match selected_device_settings(&project_devices, "device") {
                            Ok(devices) => devices,
                            Err(error) => {
                                set_status_error(&window, &toast_timer, &error.to_string());
                                return;
                            }
                        };
                    // Persist device settings to per-machine config
                    let gui_settings = GuiAudioSettings {
                        input_devices: project_device_settings.clone(),
                        output_devices: project_device_settings.clone(),
                        language: current_language(),
                    };
                    if let Err(e) = FilesystemStorage::save_gui_audio_settings(&gui_settings) {
                        log::warn!("failed to persist gui audio settings: {e}");
                    }
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        set_status_error(
                            &window,
                            &toast_timer,
                            &rust_i18n::t!("error-no-project-loaded"),
                        );
                        return;
                    };
                    session.project.device_settings =
                        project_device_settings_from_rows(project_device_settings);
                    if let Err(e) =
                        infra_cpal::apply_device_settings(&session.project.device_settings)
                    {
                        log::warn!("apply_device_settings failed: {e}");
                    }
                    if let Err(error) = sync_project_runtime(&project_runtime, session) {
                        set_status_error(&window, &toast_timer, &error.to_string());
                        return;
                    }
                    replace_project_chains(
                        &project_chains,
                        &session.project,
                        &input_chain_devices.borrow(),
                        &output_chain_devices.borrow(),
                    );
                    window.set_project_title(
                        project_title_for_path(session.project_path.as_ref(), &session.project)
                            .into(),
                    );
                    sync_project_dirty(
                        &window,
                        session,
                        &saved_project_snapshot,
                        &project_dirty,
                        auto_save,
                    );
                    clear_status(&window, &toast_timer);
                    window.set_show_project_chains(true);
                    window.set_show_chain_editor(false);
                    window.set_show_project_settings(false);
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_settings = project_settings_window.as_weak();
        project_settings_window.on_save_audio_settings(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(settings_window) = weak_settings.upgrade() else {
                return;
            };
            let project_device_settings = match selected_device_settings(&project_devices, "device")
            {
                Ok(devices) => devices,
                Err(error) => {
                    settings_window.set_status_message(error.to_string().into());
                    return;
                }
            };
            match *audio_settings_mode.borrow() {
                AudioSettingsMode::Gui => {
                    let input_devices = match selected_device_settings(&input_devices, "input") {
                        Ok(devices) => devices,
                        Err(error) => {
                            settings_window.set_status_message(error.to_string().into());
                            return;
                        }
                    };
                    let output_devices = match selected_device_settings(&output_devices, "output") {
                        Ok(devices) => devices,
                        Err(error) => {
                            settings_window.set_status_message(error.to_string().into());
                            return;
                        }
                    };
                    let settings = GuiAudioSettings {
                        input_devices,
                        output_devices,
                        language: current_language(),
                    };
                    if !settings.is_complete() {
                        settings_window.set_status_message(
                            "Selecione pelo menos um input e um output antes de continuar.".into(),
                        );
                        return;
                    }
                    match FilesystemStorage::save_gui_audio_settings(&settings) {
                        Ok(()) => {
                            settings_window.set_status_message("".into());
                            clear_status(&window, &toast_timer);
                            window.set_show_audio_settings(false);
                            let _ = settings_window.hide();
                        }
                        Err(error) => settings_window.set_status_message(error.to_string().into()),
                    }
                }
                AudioSettingsMode::Project => {
                    let gui_settings = GuiAudioSettings {
                        input_devices: project_device_settings.clone(),
                        output_devices: project_device_settings.clone(),
                        language: current_language(),
                    };
                    if let Err(e) = FilesystemStorage::save_gui_audio_settings(&gui_settings) {
                        log::warn!("failed to persist gui audio settings: {e}");
                    }
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        settings_window.set_status_message(
                            rust_i18n::t!("error-no-project-loaded").to_string().into(),
                        );
                        return;
                    };
                    session.project.device_settings =
                        project_device_settings_from_rows(project_device_settings);
                    if let Err(e) =
                        infra_cpal::apply_device_settings(&session.project.device_settings)
                    {
                        log::warn!("apply_device_settings failed: {e}");
                    }
                    if let Err(error) = sync_project_runtime(&project_runtime, session) {
                        settings_window.set_status_message(error.to_string().into());
                        return;
                    }
                    replace_project_chains(
                        &project_chains,
                        &session.project,
                        &input_chain_devices.borrow(),
                        &output_chain_devices.borrow(),
                    );
                    window.set_project_title(
                        project_title_for_path(session.project_path.as_ref(), &session.project)
                            .into(),
                    );
                    sync_project_dirty(
                        &window,
                        session,
                        &saved_project_snapshot,
                        &project_dirty,
                        auto_save,
                    );
                    settings_window.set_status_message("".into());
                    clear_status(&window, &toast_timer);
                    window.set_show_project_settings(false);
                    let _ = settings_window.hide();
                }
            }
        });
    }
}
