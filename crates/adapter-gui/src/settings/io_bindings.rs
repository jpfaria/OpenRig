//! Responsibility: wires the bindings section.
//! System / I/O bindings section wiring (#716).
//!
//! Pure wiring functions that translate Slint callback events into
//! `Command` values for the shared dispatcher. No `AppWindow` is
//! constructed in tests — every exported helper is a pure transformation
//! (LAW 1).
//!
//! The endpoint editor uses STRUCTURED pickers: a real device ComboBox, a
//! per-channel checkbox set derived from the chosen device's reported channel
//! count, and a mode ComboBox. No free text. Channel data comes ONLY from the
//! enumerated `AudioDeviceDescriptor`s threaded in from the wiring call site —
//! there is no hardcoded device or channel-count fallback.
//!
//! Bindings are identified by their `id`; endpoints by their auto-assigned
//! `name`. The wiring maintains the in-memory `AppConfig` snapshot (same
//! pattern as `settings::integrations`).
//!
//! This file holds the section's pure helpers, its Slint model projection and
//! the `wire` installer. What each gesture DOES lives on `WireCtx` in
//! `io_bindings_callbacks.rs`; the endpoint→command translation lives in
//! `io_bindings_endpoint.rs`.

use std::cell::RefCell;
use std::rc::Rc;

use domain::AudioDeviceDescriptor;
use infra_filesystem::AppConfig;
use slint::{Global, ModelRc, VecModel};

use crate::state::ProjectSession;
use crate::{AppWindow, ProjectSettingsWindow};

#[path = "io_bindings_endpoint.rs"]
pub(crate) mod io_bindings_endpoint;
pub(crate) use io_bindings_endpoint::{
    apply_channel_toggle, build_input_endpoint, build_output_endpoint, build_update_command,
    build_update_removing_endpoint, build_update_replacing_endpoint,
    build_update_with_input_endpoint, build_update_with_output_endpoint, channel_items_for_device,
    channel_mode_from_str, endpoint_prefill, next_endpoint_name,
};

#[path = "io_bindings_callbacks.rs"]
mod io_bindings_callbacks;
use io_bindings_callbacks::{install_psw_callbacks, install_window_callbacks};
// `io_bindings_tests` builds a `WireCtx` directly via `super::WireCtx`.
#[cfg(test)]
use io_bindings_callbacks::WireCtx;

#[cfg(test)]
#[path = "io_bindings_tests.rs"]
mod io_bindings_tests;
pub(crate) use super::io_bindings_helpers::build_create_command;
// `io_bindings_tests.rs` hangs off this module and reaches it through `super::`.
#[cfg(test)]
pub(crate) use super::io_bindings_helpers::surface_delete_error;
pub(crate) use super::io_bindings_helpers::{
    binding_display_name, delete_reject_message, dispatch_if_session, make_id,
    push_bindings_to_runtime, sync_snapshot_from_registry,
};
pub(crate) use super::io_bindings_models::{
    binding_names, device_list_models, project_bindings, reproject, selected_channels,
    BindingModels,
};

// ── Installer ─────────────────────────────────────────────────────────────────

/// Wire the I/O bindings section callbacks on both window surfaces.
pub fn wire(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    app_config: Rc<RefCell<AppConfig>>,
    input_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
) {
    let models = Rc::new(BindingModels {
        bindings: Rc::new(VecModel::from(project_bindings(
            &app_config.borrow().io_bindings,
        ))),
        names: Rc::new(VecModel::from(binding_names(
            &app_config.borrow().io_bindings,
        ))),
        channels: Rc::new(VecModel::default()),
    });

    // Seed the binding + channel models on both windows.
    crate::SettingsBridge::get(window).set_io_bindings(ModelRc::from(models.bindings.clone()));
    crate::SettingsBridge::get(window).set_io_binding_names(ModelRc::from(models.names.clone()));
    crate::SettingsBridge::get(window)
        .set_io_binding_channel_options(ModelRc::from(models.channels.clone()));
    crate::SettingsBridge::get(project_settings_window)
        .set_io_bindings(ModelRc::from(models.bindings.clone()));
    crate::SettingsBridge::get(project_settings_window)
        .set_io_binding_channel_options(ModelRc::from(models.channels.clone()));

    // Seed the device-list models (id + name) on both windows. Devices are
    // enumerated lazily, so this is re-run by `reseed_device_models` from the
    // Settings refresh-devices path once the hardware has been scanned.
    reseed_device_models(
        window,
        project_settings_window,
        &input_devices.borrow(),
        &output_devices.borrow(),
    );

    install_window_callbacks(
        window,
        &project_session,
        &app_config,
        &models,
        &input_devices,
        &output_devices,
    );
    install_psw_callbacks(
        project_settings_window,
        &project_session,
        &app_config,
        &models,
        &input_devices,
        &output_devices,
    );
}

/// Push freshly enumerated descriptors into the shared caches the I/O bindings
/// wiring reads from. Called from the project-settings open path so the device
/// dropdowns and channel derivation see the same populated source the audio
/// section already enumerated — without this the dropdowns stay empty because
/// the shared caches are only filled lazily on the refresh-devices button.
pub fn seed_device_caches(
    input_cache: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_cache: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    fresh_input: &[AudioDeviceDescriptor],
    fresh_output: &[AudioDeviceDescriptor],
) {
    *input_cache.borrow_mut() = fresh_input.to_vec();
    *output_cache.borrow_mut() = fresh_output.to_vec();
}

/// Rebuild the device-list models on both windows from the latest descriptors.
/// Called from the Settings refresh-devices path once the hardware is scanned
/// (devices are enumerated lazily, so the initial seed at `wire` time is empty).
pub fn reseed_device_models(
    window: &AppWindow,
    psw: &ProjectSettingsWindow,
    input_devices: &[AudioDeviceDescriptor],
    output_devices: &[AudioDeviceDescriptor],
) {
    let (in_ids, in_names) = device_list_models(input_devices);
    let (out_ids, out_names) = device_list_models(output_devices);
    crate::SettingsBridge::get(window).set_input_device_ids(ModelRc::from(in_ids.clone()));
    crate::SettingsBridge::get(window).set_input_device_names(ModelRc::from(in_names.clone()));
    crate::SettingsBridge::get(window).set_output_device_ids(ModelRc::from(out_ids.clone()));
    crate::SettingsBridge::get(window).set_output_device_names(ModelRc::from(out_names.clone()));
    crate::SettingsBridge::get(psw).set_input_device_ids(ModelRc::from(in_ids));
    crate::SettingsBridge::get(psw).set_input_device_names(ModelRc::from(in_names));
    crate::SettingsBridge::get(psw).set_output_device_ids(ModelRc::from(out_ids));
    crate::SettingsBridge::get(psw).set_output_device_names(ModelRc::from(out_names));
}
