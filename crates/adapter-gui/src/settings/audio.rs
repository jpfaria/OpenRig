//! Wiring for `on_save_audio_settings` on the main window and the standalone
//! project-settings window.
//!
//! Two callbacks, both branching on `AudioSettingsMode` (Gui = first-run setup,
//! Project = per-project device config). Shared steps: pull selected device
//! rows, persist gui-settings.yaml, write into the project session, apply
//! settings to hardware (`infra_cpal::apply_device_settings`), and resync the
//! audio runtime — which on Linux/JACK restarts jackd if sample rate or buffer
//! size changed.
//!
//! # Issue #627 — buffer size must survive a whole-config re-save
//!
//! After `Aplicar` writes device settings to disk, the in-memory `AppConfig`
//! snapshot (held by the GUI since startup) must be mirrored immediately.
//! Lifecycle events (project-open, register-recent) re-persist the WHOLE
//! in-memory snapshot via `save_app_config(&app_config.borrow())`; without the
//! mirror those events clobber the buffer the user just applied.
//! `apply_audio_override` (see below) is the seam that keeps them in sync —
//! the same pattern as `settings::paths::{apply_presets_override, …}` for #607.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Timer, VecModel};

use domain::ids::DeviceId;
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use infra_filesystem::{AppConfig, FilesystemStorage, GuiAudioDeviceSettings, GuiSystemSettings};
use project::device::DeviceSettings;

use application::command::Command;
use application::dispatcher::CommandDispatcher;

use crate::audio_devices::selected_device_settings;
use crate::default_io_binding::DEFAULT_BINDING_ID;
use crate::device_settings_wiring::wizard_create_or_update_default_binding;
use crate::helpers::{clear_status, set_status_error, set_status_warning};
use crate::project_ops::{
    build_device_settings_from_gui, project_title_for_path, sync_project_dirty,
};
use crate::project_view::replace_project_chains;
use crate::state::{AudioSettingsMode, ProjectSession};
use crate::sync_project_runtime;
use crate::{AppWindow, DeviceSelectionItem, ProjectChainItem, ProjectSettingsWindow};

/// Mirror the applied device lists into the shared in-memory `AppConfig` so
/// that a subsequent whole-config re-save (e.g. on project-open /
/// register-recent) does not clobber the user's choice with the stale
/// startup values.
///
/// Pure function (no disk I/O): the `SaveAudioSettings` dispatch already
/// persists to `config.yaml`. This call only keeps the in-memory snapshot in
/// sync — the same responsibility `apply_presets_override` / `apply_plugins_override`
/// / `apply_evaluations_override` fulfil for path overrides (#607).
pub fn apply_audio_override(
    config: &mut AppConfig,
    input_devices: &[GuiAudioDeviceSettings],
    output_devices: &[GuiAudioDeviceSettings],
) {
    config.input_devices = input_devices.to_vec();
    config.output_devices = output_devices.to_vec();
}

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
    /// Shared in-memory `AppConfig` snapshot — kept in sync with disk so that
    /// lifecycle whole-config re-saves do not clobber applied device settings (#627).
    pub app_config: Rc<RefCell<AppConfig>>,
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

/// Project mode exposes a single flat device list (it has one picker, not the
/// separate input/output pickers of GUI mode). Classify each selected device by
/// membership in the live input/output descriptor lists so the global config
/// still records the correct per-direction ids — the same physical interface
/// enumerates with a different id per direction. A device exposed in both
/// directions (shared id) is recorded in both; an id that matches neither
/// (stale/disconnected) is kept as an input so the selection is never silently
/// dropped.
fn split_device_settings_by_direction(
    selected: &[DeviceSettings],
    input_descriptors: &[AudioDeviceDescriptor],
    output_descriptors: &[AudioDeviceDescriptor],
) -> (Vec<DeviceSettings>, Vec<DeviceSettings>) {
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for device in selected {
        let is_input = input_descriptors.iter().any(|d| d.id == device.device_id.0);
        let is_output = output_descriptors
            .iter()
            .any(|d| d.id == device.device_id.0);
        if is_output {
            outputs.push(device.clone());
        }
        if is_input || !is_output {
            inputs.push(device.clone());
        }
    }
    (inputs, outputs)
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
        app_config,
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
        let app_config = app_config.clone();
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
                    let settings = GuiSystemSettings {
                        input_devices,
                        output_devices,
                        language: current_language(),
                        midi_devices: vec![],
                    };
                    if !settings.is_complete() {
                        set_status_warning(
                            &window,
                            &toast_timer,
                            "Selecione pelo menos um input e um output antes de continuar.",
                        );
                        return;
                    }
                    // #693: the settings write runs on the persist worker —
                    // the GUI thread never waits on disk; errors go to the
                    // non-blocking logger.
                    {
                        let settings_for_write = settings.clone();
                        application::persist_worker::run(move || {
                            if let Err(e) =
                                FilesystemStorage::save_gui_audio_settings(&settings_for_write)
                            {
                                log::error!("save_gui_audio_settings failed: {e}");
                            }
                        });
                    }
                    {
                        {
                            // #627: mirror the applied device lists into the shared
                            // in-memory AppConfig so a subsequent whole-config re-save
                            // (project-open / register-recent) does not clobber them.
                            apply_audio_override(
                                &mut app_config.borrow_mut(),
                                &settings.input_devices,
                                &settings.output_devices,
                            );
                            // Update in-memory device settings and resync the
                            // audio runtime so changes take effect immediately.
                            // On Linux/JACK this will restart jackd if sample
                            // rate or buffer size changed.
                            if let Some(session) = project_session.borrow_mut().as_mut() {
                                let new_device_settings = build_device_settings_from_gui(
                                    &settings.input_devices,
                                    &settings.output_devices,
                                );
                                if let Err(e) =
                                    infra_cpal::apply_device_settings(&new_device_settings)
                                {
                                    log::warn!("apply_device_settings failed: {e}");
                                }
                                // GUI mode carries a proper input/output split
                                // (two separate device models), so persist each
                                // direction's ids into its own config field.
                                let _ = session.dispatcher.dispatch(Command::SaveAudioSettings {
                                    input_devices: project_device_settings_from_rows(
                                        settings.input_devices.clone(),
                                    ),
                                    output_devices: project_device_settings_from_rows(
                                        settings.output_devices.clone(),
                                    ),
                                });
                                // #716 Task 13: create/update the "default" I/O
                                // binding when the audio wizard finishes.
                                if let (Some(input), Some(output)) = (
                                    settings.input_devices.first(),
                                    settings.output_devices.first(),
                                ) {
                                    let existing = app_config
                                        .borrow()
                                        .io_bindings
                                        .iter()
                                        .find(|b| b.id == DEFAULT_BINDING_ID)
                                        .cloned();
                                    let cmd = wizard_create_or_update_default_binding(
                                        &input.device_id,
                                        &output.device_id,
                                        existing.as_ref(),
                                    );
                                    let _ = session.dispatcher.dispatch(cmd);
                                }
                                if let Err(e) = sync_project_runtime(&project_runtime, session) {
                                    set_status_error(&window, &toast_timer, &e.to_string());
                                    return;
                                }
                            }
                            clear_status(&window, &toast_timer);
                            window.set_show_audio_settings(false);
                        }
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
                    // config.yaml persistence is owned by the dispatcher's
                    // SaveAudioSettings handler below (#581 parity) — it writes
                    // the per-direction split. No direct save_gui_audio_settings
                    // here: that collapsed input==output and corrupted re-match.
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        set_status_error(
                            &window,
                            &toast_timer,
                            &rust_i18n::t!("error-no-project-loaded"),
                        );
                        return;
                    };
                    let new_device_settings =
                        project_device_settings_from_rows(project_device_settings.clone());
                    if let Err(e) = infra_cpal::apply_device_settings(&new_device_settings) {
                        log::warn!("apply_device_settings failed: {e}");
                    }
                    let input_descriptors = input_chain_devices.borrow();
                    let output_descriptors = output_chain_devices.borrow();
                    let (input_device_settings, output_device_settings) =
                        split_device_settings_by_direction(
                            &new_device_settings,
                            &input_descriptors,
                            &output_descriptors,
                        );
                    if let Err(e) = session.dispatcher.dispatch(Command::SaveAudioSettings {
                        input_devices: input_device_settings,
                        output_devices: output_device_settings,
                    }) {
                        set_status_error(&window, &toast_timer, &e.to_string());
                        return;
                    }
                    // #627: mirror the applied device lists into the shared in-memory
                    // AppConfig using the same direction-split logic, so a subsequent
                    // whole-config re-save does not clobber the user's pick.
                    {
                        // Mirrors split_device_settings_by_direction but for GuiAudioDeviceSettings.
                        let gui_inputs: Vec<GuiAudioDeviceSettings> = project_device_settings
                            .iter()
                            .filter(|d| {
                                input_descriptors.iter().any(|id| id.id == d.device_id)
                                    || !output_descriptors.iter().any(|od| od.id == d.device_id)
                            })
                            .cloned()
                            .collect();
                        let gui_outputs: Vec<GuiAudioDeviceSettings> = project_device_settings
                            .iter()
                            .filter(|d| output_descriptors.iter().any(|od| od.id == d.device_id))
                            .cloned()
                            .collect();
                        apply_audio_override(
                            &mut app_config.borrow_mut(),
                            &gui_inputs,
                            &gui_outputs,
                        );
                    }
                    if let Err(error) = sync_project_runtime(&project_runtime, session) {
                        set_status_error(&window, &toast_timer, &error.to_string());
                        return;
                    }
                    replace_project_chains(
                        &project_chains,
                        &*session.project.borrow(),
                        &input_descriptors,
                        &output_descriptors,
                        &[],
                    );
                    window.set_project_title(
                        project_title_for_path(
                            session.project_path.as_ref(),
                            &*session.project.borrow(),
                        )
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
                    window.set_show_settings(false);
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_settings = project_settings_window.as_weak();
        let app_config = app_config.clone();
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
                    let settings = GuiSystemSettings {
                        input_devices,
                        output_devices,
                        language: current_language(),
                        midi_devices: vec![],
                    };
                    if !settings.is_complete() {
                        settings_window.set_status_message(
                            "Selecione pelo menos um input e um output antes de continuar.".into(),
                        );
                        return;
                    }
                    // #693: write on the persist worker; errors to the log.
                    {
                        let settings_for_write = settings.clone();
                        application::persist_worker::run(move || {
                            if let Err(e) =
                                FilesystemStorage::save_gui_audio_settings(&settings_for_write)
                            {
                                log::error!("save_gui_audio_settings failed: {e}");
                            }
                        });
                    }
                    // #627: mirror into the shared in-memory AppConfig.
                    apply_audio_override(
                        &mut app_config.borrow_mut(),
                        &settings.input_devices,
                        &settings.output_devices,
                    );
                    // #513: Apply restarts the audio runtime but keeps
                    // the Settings window open. User dismisses via FECHAR.
                    settings_window.set_status_message("".into());
                    clear_status(&window, &toast_timer);
                }
                AudioSettingsMode::Project => {
                    // config.yaml persistence is owned by the dispatcher's
                    // SaveAudioSettings handler below (#581 parity) — it writes
                    // the per-direction split. No direct save_gui_audio_settings
                    // here: that collapsed input==output and corrupted re-match.
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        settings_window.set_status_message(
                            rust_i18n::t!("error-no-project-loaded").to_string().into(),
                        );
                        return;
                    };
                    let new_device_settings =
                        project_device_settings_from_rows(project_device_settings.clone());
                    if let Err(e) = infra_cpal::apply_device_settings(&new_device_settings) {
                        log::warn!("apply_device_settings failed: {e}");
                    }
                    let input_descriptors = input_chain_devices.borrow();
                    let output_descriptors = output_chain_devices.borrow();
                    let (input_device_settings, output_device_settings) =
                        split_device_settings_by_direction(
                            &new_device_settings,
                            &input_descriptors,
                            &output_descriptors,
                        );
                    if let Err(e) = session.dispatcher.dispatch(Command::SaveAudioSettings {
                        input_devices: input_device_settings,
                        output_devices: output_device_settings,
                    }) {
                        settings_window.set_status_message(e.to_string().into());
                        return;
                    }
                    // #627: mirror into the shared in-memory AppConfig.
                    {
                        let gui_inputs: Vec<GuiAudioDeviceSettings> = project_device_settings
                            .iter()
                            .filter(|d| {
                                input_descriptors.iter().any(|id| id.id == d.device_id)
                                    || !output_descriptors.iter().any(|od| od.id == d.device_id)
                            })
                            .cloned()
                            .collect();
                        let gui_outputs: Vec<GuiAudioDeviceSettings> = project_device_settings
                            .iter()
                            .filter(|d| output_descriptors.iter().any(|od| od.id == d.device_id))
                            .cloned()
                            .collect();
                        apply_audio_override(
                            &mut app_config.borrow_mut(),
                            &gui_inputs,
                            &gui_outputs,
                        );
                    }
                    if let Err(error) = sync_project_runtime(&project_runtime, session) {
                        settings_window.set_status_message(error.to_string().into());
                        return;
                    }
                    replace_project_chains(
                        &project_chains,
                        &*session.project.borrow(),
                        &input_descriptors,
                        &output_descriptors,
                        &[],
                    );
                    window.set_project_title(
                        project_title_for_path(
                            session.project_path.as_ref(),
                            &*session.project.borrow(),
                        )
                        .into(),
                    );
                    sync_project_dirty(
                        &window,
                        session,
                        &saved_project_snapshot,
                        &project_dirty,
                        auto_save,
                    );
                    // #513: keep window open on Apply.
                    settings_window.set_status_message("".into());
                    clear_status(&window, &toast_timer);
                }
            }
        });
    }
}
