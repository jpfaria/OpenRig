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

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, IoBindingCommand};
use application::dispatcher::CommandDispatcher;
use domain::io_binding::{IoBinding, IoEndpoint};
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use infra_filesystem::AppConfig;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::state::ProjectSession;
use crate::{AppWindow, ChannelOptionItem, IoBindingModel, IoEndpointModel, ProjectSettingsWindow};

#[path = "io_bindings_endpoint.rs"]
mod io_bindings_endpoint;
pub(crate) use io_bindings_endpoint::{
    apply_channel_toggle, build_input_endpoint, build_output_endpoint,
    build_update_command, build_update_removing_endpoint, build_update_replacing_endpoint,
    build_update_with_input_endpoint, build_update_with_output_endpoint, channel_items_for_device,
    channel_mode_from_str, endpoint_prefill, next_endpoint_name,
};

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

/// Apply an `UpdateIoBinding` command produced by the endpoint builders: store
/// the mutated binding back into the in-memory config slot and dispatch it.
fn apply_binding_command(
    slot: &mut IoBinding,
    ps: &Rc<RefCell<Option<ProjectSession>>>,
    cmd: Command,
) {
    if let Command::IoBinding(IoBindingCommand::UpdateIoBinding { binding }) = &cmd {
        *slot = binding.clone();
    }
    dispatch_if_session(ps, cmd);
}

/// Name for a new binding: the typed name, or a sequential default ("I/O N").
fn binding_display_name(name: &str, cfg: &Rc<RefCell<AppConfig>>) -> String {
    let trimmed = name.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    format!("I/O {}", cfg.borrow().io_bindings.len() + 1)
}

/// Mirror the edited registry into the open session so bound chains resolve
/// against the latest registry on the next runtime sync.
fn mirror_bindings_to_session(
    ps: &Rc<RefCell<Option<ProjectSession>>>,
    cfg: &Rc<RefCell<AppConfig>>,
) {
    if let Some(session) = ps.borrow().as_ref() {
        *session.io_bindings.borrow_mut() = cfg.borrow().io_bindings.clone();
    }
}

/// #716 (AUDIO-CRITICAL): push the edited registry straight into the live
/// runtime controller so a chain that is ALREADY running re-resolves its
/// device endpoints against the user's latest binding edit on the next sync.
/// Without this, a binding change only reaches the controller on the next
/// cold start; a running rig keeps the stale registry.
fn push_bindings_to_runtime(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    cfg: &Rc<RefCell<AppConfig>>,
) {
    if let Some(controller) = runtime.borrow_mut().as_mut() {
        controller.set_io_bindings(cfg.borrow().io_bindings.clone());
    }
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

fn project_bindings(cfg: &AppConfig) -> Vec<IoBindingModel> {
    cfg.io_bindings.iter().map(binding_model).collect()
}

fn binding_names(cfg: &AppConfig) -> Vec<SharedString> {
    cfg.io_bindings
        .iter()
        .map(|b| SharedString::from(b.name.as_str()))
        .collect()
}

/// Re-project the binding list into the shared Slint models after any mutation.
fn reproject(models: &BindingModels, cfg: &Rc<RefCell<AppConfig>>) {
    let config = cfg.borrow();
    models.bindings.set_vec(project_bindings(&config));
    models.names.set_vec(binding_names(&config));
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
    project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
) {
    let models = Rc::new(BindingModels {
        bindings: Rc::new(VecModel::from(project_bindings(&app_config.borrow()))),
        names: Rc::new(VecModel::from(binding_names(&app_config.borrow()))),
        channels: Rc::new(VecModel::default()),
    });

    // Seed the binding + channel models on both windows.
    window.set_io_bindings(ModelRc::from(models.bindings.clone()));
    window.set_io_binding_names(ModelRc::from(models.names.clone()));
    window.set_io_binding_channel_options(ModelRc::from(models.channels.clone()));
    project_settings_window.set_io_bindings(ModelRc::from(models.bindings.clone()));
    project_settings_window.set_io_binding_channel_options(ModelRc::from(models.channels.clone()));

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
        &project_runtime,
    );
    install_psw_callbacks(
        project_settings_window,
        &project_session,
        &app_config,
        &models,
        &input_devices,
        &output_devices,
        &project_runtime,
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
    window.set_input_device_ids(ModelRc::from(in_ids.clone()));
    window.set_input_device_names(ModelRc::from(in_names.clone()));
    window.set_output_device_ids(ModelRc::from(out_ids.clone()));
    window.set_output_device_names(ModelRc::from(out_names.clone()));
    psw.set_input_device_ids(ModelRc::from(in_ids));
    psw.set_input_device_names(ModelRc::from(in_names));
    psw.set_output_device_ids(ModelRc::from(out_ids));
    psw.set_output_device_names(ModelRc::from(out_names));
}

/// Shared closure state for a single window's callbacks.
struct WireCtx {
    ps: Rc<RefCell<Option<ProjectSession>>>,
    cfg: Rc<RefCell<AppConfig>>,
    models: Rc<BindingModels>,
    input_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    /// #716: the live runtime controller (if any). Edited bindings are pushed
    /// here so a running rig picks them up immediately.
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl WireCtx {
    /// Mirror the edited registry into the open session AND the live runtime
    /// controller (#716). Called after every binding/endpoint mutation.
    fn propagate_bindings(&self) {
        mirror_bindings_to_session(&self.ps, &self.cfg);
        push_bindings_to_runtime(&self.runtime, &self.cfg);
    }

    fn create_binding(&self, name: &str) -> SharedString {
        let display = binding_display_name(name, &self.cfg);
        let id = make_id(&display);
        let binding = IoBinding {
            id: id.clone(),
            name: display,
            inputs: vec![],
            outputs: vec![],
        };
        dispatch_if_session(&self.ps, build_create_command(binding.clone()));
        self.cfg.borrow_mut().io_bindings.push(binding);
        self.propagate_bindings();
        reproject(&self.models, &self.cfg);
        SharedString::from(id)
    }

    fn delete_binding(&self, id: &str) -> SharedString {
        let msg = delete_reject_message(&self.ps, id);
        if msg.is_empty() {
            self.cfg.borrow_mut().io_bindings.retain(|b| b.id != id);
            self.propagate_bindings();
            reproject(&self.models, &self.cfg);
        }
        SharedString::from(msg)
    }

    fn rename_binding(&self, id: &str, new_name: &str) {
        {
            let mut config = self.cfg.borrow_mut();
            if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id) {
                b.name = new_name.to_string();
                dispatch_if_session(&self.ps, build_update_command(b.clone()));
            }
        }
        self.propagate_bindings();
        reproject(&self.models, &self.cfg);
    }

    /// Rebuild the channel checkboxes from the chosen device's channel count.
    fn device_changed(&self, is_input: bool, device_id: &str) {
        let devices = if is_input {
            self.input_devices.borrow()
        } else {
            self.output_devices.borrow()
        };
        let items = channel_items_for_device(device_id, &devices, &[]);
        self.models.channels.set_vec(items);
    }

    fn toggle_channel(&self, index: i32, selected: bool, mode: &str) {
        let model = &self.models.channels;
        let current: Vec<ChannelOptionItem> = model.iter().collect();
        let updated = apply_channel_toggle(&current, index, selected, channel_mode_from_str(mode));
        model.set_vec(updated);
    }

    /// Add (or, when `edit_name` is non-empty, replace) an endpoint on the
    /// binding. The replace path keeps the endpoint's name and position so an
    /// edit updates the row in place instead of appending a duplicate.
    fn add_endpoint(&self, id: &str, device_id: &str, mode: &str, is_input: bool, edit_name: &str) {
        let channels = selected_channels(&self.models.channels);
        if channels.is_empty() {
            return;
        }
        let parsed_mode = channel_mode_from_str(mode);
        {
            let mut config = self.cfg.borrow_mut();
            if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id) {
                let cmd = if !edit_name.is_empty() {
                    // Edit: replace the endpoint in place, keeping its name.
                    let ep = if is_input {
                        build_input_endpoint(edit_name, device_id, channels, parsed_mode)
                    } else {
                        build_output_endpoint(edit_name, device_id, channels, parsed_mode)
                    };
                    build_update_replacing_endpoint(b.clone(), edit_name, ep, is_input)
                } else {
                    let name = next_endpoint_name(
                        if is_input {
                            b.inputs.len()
                        } else {
                            b.outputs.len()
                        },
                        is_input,
                    );
                    if is_input {
                        let ep = build_input_endpoint(&name, device_id, channels, parsed_mode);
                        build_update_with_input_endpoint(b.clone(), ep)
                    } else {
                        let ep = build_output_endpoint(&name, device_id, channels, parsed_mode);
                        build_update_with_output_endpoint(b.clone(), ep)
                    }
                };
                apply_binding_command(b, &self.ps, cmd);
            }
        }
        self.models.channels.set_vec(Vec::new());
        self.propagate_bindings();
        reproject(&self.models, &self.cfg);
    }

    /// Seed the channel model + prefill props for editing an existing endpoint,
    /// and return the (device_index, mode_index) the form should preselect.
    fn edit_endpoint(&self, id: &str, ep_name: &str, is_input: bool) -> (i32, i32) {
        let devices = if is_input {
            self.input_devices.borrow()
        } else {
            self.output_devices.borrow()
        };
        let config = self.cfg.borrow();
        let Some(binding) = config.io_bindings.iter().find(|b| b.id == id) else {
            return (-1, 0);
        };
        let Some(prefill) = endpoint_prefill(binding, ep_name, is_input, &devices) else {
            return (-1, 0);
        };
        self.models.channels.set_vec(prefill.channel_items);
        let mode_index = match prefill.mode {
            domain::io_binding::ChannelMode::Mono => 0,
            domain::io_binding::ChannelMode::Stereo => 1,
            domain::io_binding::ChannelMode::DualMono => 2,
        };
        (prefill.device_index, mode_index)
    }

    fn remove_endpoint(&self, id: &str, ep_name: &str, is_input: bool) {
        {
            let mut config = self.cfg.borrow_mut();
            if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id) {
                let cmd = build_update_removing_endpoint(b.clone(), ep_name, is_input);
                apply_binding_command(b, &self.ps, cmd);
            }
        }
        self.propagate_bindings();
        reproject(&self.models, &self.cfg);
    }
}

fn make_ctx(
    ps: &Rc<RefCell<Option<ProjectSession>>>,
    cfg: &Rc<RefCell<AppConfig>>,
    models: &Rc<BindingModels>,
    input_devices: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_devices: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Rc<WireCtx> {
    Rc::new(WireCtx {
        ps: Rc::clone(ps),
        cfg: Rc::clone(cfg),
        models: Rc::clone(models),
        input_devices: Rc::clone(input_devices),
        output_devices: Rc::clone(output_devices),
        runtime: Rc::clone(runtime),
    })
}

fn install_window_callbacks(
    window: &AppWindow,
    ps: &Rc<RefCell<Option<ProjectSession>>>,
    cfg: &Rc<RefCell<AppConfig>>,
    models: &Rc<BindingModels>,
    input_devices: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_devices: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) {
    let ctx = make_ctx(ps, cfg, models, input_devices, output_devices, runtime);

    let c = ctx.clone();
    window.on_create_io_binding(move |name| c.create_binding(name.as_str()));
    let c = ctx.clone();
    window.on_delete_io_binding(move |id| c.delete_binding(id.as_str()));
    let c = ctx.clone();
    window.on_rename_io_binding(move |id, n| c.rename_binding(id.as_str(), n.as_str()));
    let c = ctx.clone();
    window.on_endpoint_device_changed(move |_id, is_input, dev| {
        c.device_changed(is_input, dev.as_str())
    });
    let c = ctx.clone();
    window.on_toggle_endpoint_channel(move |idx, sel, mode| {
        c.toggle_channel(idx, sel, mode.as_str())
    });
    let c = ctx.clone();
    window.on_add_input_endpoint(move |id, dev, mode, en| {
        c.add_endpoint(id.as_str(), dev.as_str(), mode.as_str(), true, en.as_str())
    });
    let c = ctx.clone();
    window.on_add_output_endpoint(move |id, dev, mode, en| {
        c.add_endpoint(id.as_str(), dev.as_str(), mode.as_str(), false, en.as_str())
    });
    let c = ctx.clone();
    window.on_remove_endpoint(move |id, en, inp| c.remove_endpoint(id.as_str(), en.as_str(), inp));
    let c = ctx.clone();
    let weak = window.as_weak();
    window.on_edit_endpoint(move |id, en, inp| {
        let (dev_idx, mode_idx) = c.edit_endpoint(id.as_str(), en.as_str(), inp);
        if let Some(w) = weak.upgrade() {
            w.set_io_edit_prefill_device_index(dev_idx);
            w.set_io_edit_prefill_mode_index(mode_idx);
        }
    });
}

fn install_psw_callbacks(
    psw: &ProjectSettingsWindow,
    ps: &Rc<RefCell<Option<ProjectSession>>>,
    cfg: &Rc<RefCell<AppConfig>>,
    models: &Rc<BindingModels>,
    input_devices: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_devices: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) {
    let ctx = make_ctx(ps, cfg, models, input_devices, output_devices, runtime);

    let c = ctx.clone();
    psw.on_create_io_binding(move |name| c.create_binding(name.as_str()));
    let c = ctx.clone();
    psw.on_delete_io_binding(move |id| c.delete_binding(id.as_str()));
    let c = ctx.clone();
    psw.on_rename_io_binding(move |id, n| c.rename_binding(id.as_str(), n.as_str()));
    let c = ctx.clone();
    psw.on_endpoint_device_changed(move |_id, is_input, dev| {
        c.device_changed(is_input, dev.as_str())
    });
    let c = ctx.clone();
    psw.on_toggle_endpoint_channel(move |idx, sel, mode| c.toggle_channel(idx, sel, mode.as_str()));
    let c = ctx.clone();
    psw.on_add_input_endpoint(move |id, dev, mode, en| {
        c.add_endpoint(id.as_str(), dev.as_str(), mode.as_str(), true, en.as_str())
    });
    let c = ctx.clone();
    psw.on_add_output_endpoint(move |id, dev, mode, en| {
        c.add_endpoint(id.as_str(), dev.as_str(), mode.as_str(), false, en.as_str())
    });
    let c = ctx.clone();
    psw.on_remove_endpoint(move |id, en, inp| c.remove_endpoint(id.as_str(), en.as_str(), inp));
    let c = ctx.clone();
    let weak = psw.as_weak();
    psw.on_edit_endpoint(move |id, en, inp| {
        let (dev_idx, mode_idx) = c.edit_endpoint(id.as_str(), en.as_str(), inp);
        if let Some(w) = weak.upgrade() {
            w.set_io_edit_prefill_device_index(dev_idx);
            w.set_io_edit_prefill_mode_index(mode_idx);
        }
    });
}
