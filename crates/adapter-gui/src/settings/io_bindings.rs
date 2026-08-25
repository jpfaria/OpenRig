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

use application::command::{Command, IoBindingCommand};
use domain::io_binding::{IoBinding, IoEndpoint};
use domain::AudioDeviceDescriptor;
use infra_filesystem::AppConfig;
use slint::{Model, ModelRc, SharedString, VecModel, Global};

use crate::state::ProjectSession;
use crate::{AppWindow, ChannelOptionItem, IoBindingModel, IoEndpointModel, ProjectSettingsWindow};

#[path = "io_bindings_endpoint.rs"]
mod io_bindings_endpoint;
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

// ── Pure helpers (testable without AppWindow) ─────────────────────────────────

/// Build an `IoBindingCommand::CreateIoBinding` for a new binding.
pub(crate) fn build_create_command(binding: IoBinding) -> Command {
    Command::IoBinding(IoBindingCommand::CreateIoBinding { binding })
}

/// Convert a dispatcher reject `Err` into a display string for the UI.
/// Leaves `list` unchanged — the delete was rejected, so no mutation.
pub(crate) fn surface_delete_error(err: &anyhow::Error, _list: &mut Vec<IoBinding>) -> String {
    err.to_string()
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn dispatch_if_session(ps: &Rc<RefCell<Option<ProjectSession>>>, cmd: Command) {
    if let Some(session) = ps.borrow().as_ref() {
        let _ = session.dispatcher.dispatch(cmd);
    }
}

/// Name for a new binding: the typed name, or a sequential default ("I/O N").
fn binding_display_name(name: &str, bindings: &[IoBinding]) -> String {
    let trimmed = name.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    format!("I/O {}", bindings.len() + 1)
}

/// Refresh the GUI's `AppConfig` snapshot FROM the registry the dispatcher owns
/// (#127).
///
/// The mirror used to run the other way — snapshot INTO the registry — so a
/// binding created from MCP/gRPC landed in the registry and was then wiped by
/// the next click on this screen: the command reported success and a GUI code
/// path silently undid it. Inverting it makes the dispatcher's registry the
/// single source of truth, and the other GUI readers of `AppConfig.io_bindings`
/// (chain CRUD, device refresh, the audio section) see those edits too.
fn sync_snapshot_from_registry(
    ps: &Rc<RefCell<Option<ProjectSession>>>,
    cfg: &Rc<RefCell<AppConfig>>,
) {
    let registry = ps
        .borrow()
        .as_ref()
        .map(|session| session.io_bindings.borrow().clone());
    if let Some(bindings) = registry {
        cfg.borrow_mut().io_bindings = bindings;
    }
}

/// #716 (AUDIO-CRITICAL), as a Command since #127: install the edited registry
/// into the live runtime so an ALREADY-RUNNING chain re-resolves its device
/// endpoints against the latest edit instead of waiting for the next cold
/// start. The GUI used to call the controller directly, which left MCP/gRPC
/// with no way to reach the live registry at all.
fn push_bindings_to_runtime(ps: &Rc<RefCell<Option<ProjectSession>>>) {
    dispatch_if_session(ps, Command::IoBinding(IoBindingCommand::SetIoBindings));
}

fn delete_reject_message(ps: &Rc<RefCell<Option<ProjectSession>>>, id: &str) -> String {
    let cmd = Command::IoBinding(IoBindingCommand::DeleteIoBinding { id: id.to_string() });
    if let Some(session) = ps.borrow().as_ref() {
        match session.dispatcher.dispatch(cmd) {
            Ok(_) => String::new(),
            Err(e) => {
                let mut dummy: Vec<IoBinding> = Vec::new();
                surface_delete_error(&e, &mut dummy)
            }
        }
    } else {
        String::new()
    }
}

/// Generate a slug-style id from the binding name + a small hash.
fn make_id(name: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;

    let mut h = DefaultHasher::new();
    name.hash(&mut h);
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos()
        .hash(&mut h);

    let slug: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .map(|c| {
            if c == ' ' {
                '-'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .take(24)
        .collect();

    format!("{slug}-{:x}", h.finish() & 0xffff)
}

// ── Slint model projection ────────────────────────────────────────────────────

/// Shared Slint models the section renders, set on both window surfaces.
struct BindingModels {
    bindings: Rc<VecModel<IoBindingModel>>,
    /// Chain-level endpoint picker still consumes flat names on the main window.
    names: Rc<VecModel<SharedString>>,
    /// Channel checkboxes for the active add-row (rebuilt on device change).
    channels: Rc<VecModel<ChannelOptionItem>>,
}

fn endpoint_model(ep: &IoEndpoint) -> IoEndpointModel {
    use crate::ui_state::channels_label;
    IoEndpointModel {
        name: ep.name.as_str().into(),
        device_label: ep.device_id.0.as_str().into(),
        mode: io_bindings_endpoint::mode_label(ep.mode).into(),
        channels_label: channels_label(&ep.channels).into(),
    }
}

fn binding_model(b: &IoBinding) -> IoBindingModel {
    let inputs: Vec<IoEndpointModel> = b.inputs.iter().map(endpoint_model).collect();
    let outputs: Vec<IoEndpointModel> = b.outputs.iter().map(endpoint_model).collect();
    IoBindingModel {
        id: b.id.as_str().into(),
        name: b.name.as_str().into(),
        inputs: ModelRc::from(Rc::new(VecModel::from(inputs))),
        outputs: ModelRc::from(Rc::new(VecModel::from(outputs))),
    }
}

fn project_bindings(bindings: &[IoBinding]) -> Vec<IoBindingModel> {
    bindings.iter().map(binding_model).collect()
}

fn binding_names(bindings: &[IoBinding]) -> Vec<SharedString> {
    bindings
        .iter()
        .map(|b| SharedString::from(b.name.as_str()))
        .collect()
}

/// Re-project the binding list into the shared Slint models after any mutation.
fn reproject(models: &BindingModels, bindings: &[IoBinding]) {
    models.bindings.set_vec(project_bindings(bindings));
    models.names.set_vec(binding_names(bindings));
}

/// Build the (id, name) device-list models for one side from the live
/// descriptors. Empty when devices haven't been enumerated yet.
fn device_list_models(
    devices: &[AudioDeviceDescriptor],
) -> (Rc<VecModel<SharedString>>, Rc<VecModel<SharedString>>) {
    let ids = devices
        .iter()
        .map(|d| SharedString::from(d.id.as_str()))
        .collect::<Vec<_>>();
    let names = devices
        .iter()
        .map(|d| SharedString::from(d.name.as_str()))
        .collect::<Vec<_>>();
    (Rc::new(VecModel::from(ids)), Rc::new(VecModel::from(names)))
}

/// Currently-selected 0-based channel indices in the shared channel model.
fn selected_channels(channels: &Rc<VecModel<ChannelOptionItem>>) -> Vec<usize> {
    channels
        .iter()
        .filter(|c| c.selected)
        .map(|c| c.index as usize)
        .collect()
}

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
    crate::SettingsBridge::get(window).set_io_binding_channel_options(ModelRc::from(models.channels.clone()));
    crate::SettingsBridge::get(project_settings_window).set_io_bindings(ModelRc::from(models.bindings.clone()));
    crate::SettingsBridge::get(project_settings_window).set_io_binding_channel_options(ModelRc::from(models.channels.clone()));

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
