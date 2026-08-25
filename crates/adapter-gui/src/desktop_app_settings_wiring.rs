//! Responsibility: wires the Settings screen's sections to their callbacks.
//!
//! Both surfaces at once: the inline (fullscreen) settings page on the main
//! window and the standalone `ProjectSettingsWindow` — every section installs
//! on both, so a setting behaves the same wherever the user opened it (#513).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ModelRc, Timer, VecModel, Global};

use crate::state::{AudioSettingsMode, ProjectSession};
use crate::{AppWindow, ProjectSettingsWindow};

pub(crate) struct SettingsWiringDeps {
    pub input_devices: Rc<VecModel<crate::DeviceSelectionItem>>,
    pub output_devices: Rc<VecModel<crate::DeviceSelectionItem>>,
    pub project_devices: Rc<VecModel<crate::DeviceSelectionItem>>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<crate::ProjectChainItem>>,
    pub chain_input_device_options: Rc<VecModel<slint::SharedString>>,
    pub chain_output_device_options: Rc<VecModel<slint::SharedString>>,
    pub input_chain_devices: Rc<RefCell<Vec<domain::AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<domain::AudioDeviceDescriptor>>>,
    pub audio_settings_mode: Rc<RefCell<AudioSettingsMode>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub toast_timer: Rc<Timer>,
    pub app_config: Rc<RefCell<infra_filesystem::AppConfig>>,
    pub auto_save: bool,
}

pub(crate) fn wire(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    deps: SettingsWiringDeps,
) {
    let SettingsWiringDeps {
        input_devices,
        output_devices,
        project_devices,
        project_session,
        project_chains,
        chain_input_device_options,
        chain_output_device_options,
        input_chain_devices,
        output_chain_devices,
        audio_settings_mode,
        saved_project_snapshot,
        project_dirty,
        toast_timer,
        app_config,
        auto_save,
    } = deps;
    // --- Device settings callbacks (extracted to device_settings_wiring) ---
    crate::device_settings_wiring::wire(
        window,
        project_settings_window,
        crate::device_settings_wiring::DeviceSettingsCtx {
            input_devices: input_devices.clone(),
            output_devices: output_devices.clone(),
            project_devices: project_devices.clone(),
        },
    );
    // Refresh devices — re-enumerates audio interfaces after a USB hot-swap.
    // Wired on both the standalone settings window and the inline (fullscreen)
    // settings page on the main window. Safe to call: the underlying
    // enumeration runs in the UI thread and is rate-limited by user clicks
    // (no periodic polling — that triggered scarlett2_notify freezes on
    // the Orange Pi USB-C OTG port).
    // --- Refresh devices callbacks (extracted to device_refresh_wiring) ---
    crate::device_refresh_wiring::wire(
        window,
        project_settings_window,
        crate::device_refresh_wiring::DeviceRefreshCtx {
            project_session: project_session.clone(),
            project_devices: project_devices.clone(),
            chain_input_device_options: chain_input_device_options.clone(),
            chain_output_device_options: chain_output_device_options.clone(),
            toast_timer: toast_timer.clone(),
            app_config: app_config.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
        },
    );
    // --- Audio wizard step nav callbacks (extracted to audio_wizard_wiring) ---
    crate::audio_wizard_wiring::wire(
        window,
        crate::audio_wizard_wiring::AudioWizardCtx {
            input_devices: input_devices.clone(),
            toast_timer: toast_timer.clone(),
        },
    );
    // --- Audio settings save callbacks (extracted to settings::audio) ---
    crate::settings::audio::wire(
        window,
        project_settings_window,
        crate::settings::audio::AudioSettingsSaveCtx {
            input_devices: input_devices.clone(),
            output_devices: output_devices.clone(),
            project_devices: project_devices.clone(),
            audio_settings_mode: audio_settings_mode.clone(),
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            toast_timer: toast_timer.clone(),
            auto_save,
            app_config: app_config.clone(),
        },
    );
    // --- System / MIDI devices section (#513) ---
    // Seed the in-memory row list from the persisted AppConfig and bind
    // it to the Slint model the section reads from. Each user edit
    // dispatches `SaveMidiDevices` (when a session is loaded) and
    // persists into config.yaml in the same callback — see
    // `crate::settings::midi_devices` for the rationale.
    let midi_device_rows: Rc<RefCell<Vec<infra_filesystem::MidiDeviceSelection>>> =
        Rc::new(RefCell::new(
            infra_filesystem::FilesystemStorage::load_app_config()
                .ok()
                .map(|c| c.midi_devices)
                .unwrap_or_default(),
        ));
    let midi_device_model: Rc<VecModel<crate::MidiDeviceRow>> = Rc::new(VecModel::default());
    crate::settings::midi_devices::replace_model(&midi_device_model, &midi_device_rows.borrow());
    crate::settings::midi_devices::install(
        window,
        project_session.clone(),
        midi_device_rows.clone(),
        midi_device_model.clone(),
    );
    crate::settings::midi_devices::install_secondary(
        project_settings_window,
        project_session.clone(),
        midi_device_rows.clone(),
        midi_device_model.clone(),
    );
    crate::SettingsBridge::get(window).set_midi_devices(ModelRc::from(midi_device_model.clone()));
    crate::SettingsBridge::get(project_settings_window).set_midi_devices(ModelRc::from(midi_device_model.clone()));
    // --- Project / Metadata section (#513) ---
    let last_dispatched_name: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    crate::settings::project_meta::install(
        window,
        project_session.clone(),
        last_dispatched_name.clone(),
    );
    crate::settings::project_meta::install_secondary(
        project_settings_window,
        project_session.clone(),
        last_dispatched_name.clone(),
    );
    // --- System / Paths section (#513) ---
    crate::settings::paths::install(window, project_session.clone(), app_config.clone());
    crate::settings::paths::install_secondary(
        project_settings_window,
        project_session.clone(),
        app_config.clone(),
    );
    crate::settings::paths::seed_initial(window);
    crate::settings::paths::seed_initial_secondary(project_settings_window);
}
