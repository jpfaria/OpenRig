//! Responsibility: handles the interaction inside the bindings section.
//! Interactive behaviour of the System / I/O bindings section (#716).
//!
//! `WireCtx` is the shared closure state one window surface hands to every one
//! of its callbacks, and it owns what each gesture actually does: create,
//! rename and delete a binding, rebuild the channel checkboxes when the device
//! changes, and add/edit/remove an endpoint. The two window surfaces
//! (`AppWindow` and `ProjectSettingsWindow`) expose the same callbacks, so each
//! gets its own context and the same set of closures.
//!
//! Split out of `io_bindings.rs`, which keeps the section's pure helpers and
//! its Slint model projection.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, IoBindingCommand};
use domain::io_binding::IoBinding;
use domain::AudioDeviceDescriptor;
use infra_filesystem::AppConfig;
use slint::{ComponentHandle, Global, Model, SharedString};

use super::{
    apply_channel_toggle, binding_display_name, build_create_command, build_input_endpoint,
    build_output_endpoint, build_update_command, build_update_removing_endpoint,
    build_update_replacing_endpoint, build_update_with_input_endpoint,
    build_update_with_output_endpoint, channel_items_for_device, channel_mode_from_str,
    delete_reject_message, dispatch_if_session, endpoint_prefill, make_id, next_endpoint_name,
    push_bindings_to_runtime, reproject, selected_channels, sync_snapshot_from_registry,
    BindingModels,
};
use crate::state::ProjectSession;
use crate::{AppWindow, ChannelOptionItem, ProjectSettingsWindow};

/// Shared closure state for a single window's callbacks.
///
/// `pub(super)` so `io_bindings_tests` can build one directly and drive a
/// gesture without an `AppWindow` (LAW 1).
pub(super) struct WireCtx {
    pub(super) ps: Rc<RefCell<Option<ProjectSession>>>,
    pub(super) cfg: Rc<RefCell<AppConfig>>,
    pub(super) models: Rc<BindingModels>,
    pub(super) input_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub(super) output_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
}

impl WireCtx {
    /// The effective registry this screen reads: the one the dispatcher owns
    /// when a project is open (#127 — its command handlers mutate it and
    /// persist it), else the GUI's own `AppConfig` snapshot.
    ///
    /// Returned by value on purpose: nothing may stay borrowed across a
    /// `dispatch`, because the handler borrows the very same registry.
    fn bindings(&self) -> Vec<IoBinding> {
        match self.ps.borrow().as_ref() {
            Some(session) => session.io_bindings.borrow().clone(),
            None => self.cfg.borrow().io_bindings.clone(),
        }
    }

    /// Apply `mutate` locally ONLY when there is no dispatcher to write
    /// through. With a project open the command handler owns the registry and
    /// has already applied the edit — writing it again from here is what
    /// clobbered edits issued by other transports.
    fn apply_without_dispatcher(&self, mutate: impl FnOnce(&mut Vec<IoBinding>)) {
        if self.ps.borrow().is_some() {
            return;
        }
        mutate(&mut self.cfg.borrow_mut().io_bindings);
    }

    /// Dispatch an `UpdateIoBinding` built by the endpoint helpers, and mirror
    /// it locally only in the no-dispatcher case.
    fn apply_binding_command(&self, cmd: Command) {
        if let Command::IoBinding(IoBindingCommand::UpdateIoBinding { binding }) = &cmd {
            let binding = binding.clone();
            self.apply_without_dispatcher(move |list| {
                match list.iter().position(|b| b.id == binding.id) {
                    Some(pos) => list[pos] = binding,
                    None => list.push(binding),
                }
            });
        }
        dispatch_if_session(&self.ps, cmd);
    }

    /// Refresh the GUI snapshot from the dispatcher's registry, then install
    /// that registry into the live runtime (#716). Called after every
    /// binding/endpoint mutation.
    fn propagate_bindings(&self) {
        sync_snapshot_from_registry(&self.ps, &self.cfg);
        push_bindings_to_runtime(&self.ps);
    }

    /// Re-render the list from the effective registry (never from a snapshot
    /// captured before the command ran).
    fn refresh_models(&self) {
        reproject(&self.models, &self.bindings());
    }

    pub(super) fn create_binding(&self, name: &str) -> SharedString {
        let display = binding_display_name(name, &self.bindings());
        let id = make_id(&display);
        let binding = IoBinding {
            id: id.clone(),
            name: display,
            inputs: vec![],
            outputs: vec![],
        };
        dispatch_if_session(&self.ps, build_create_command(binding.clone()));
        self.apply_without_dispatcher(move |list| list.push(binding));
        self.propagate_bindings();
        self.refresh_models();
        SharedString::from(id)
    }

    fn delete_binding(&self, id: &str) -> SharedString {
        let msg = delete_reject_message(&self.ps, id);
        if msg.is_empty() {
            let id = id.to_string();
            self.apply_without_dispatcher(move |list| list.retain(|b| b.id != id));
            self.propagate_bindings();
            self.refresh_models();
        }
        SharedString::from(msg)
    }

    fn rename_binding(&self, id: &str, new_name: &str) {
        let Some(mut binding) = self.bindings().into_iter().find(|b| b.id == id) else {
            return;
        };
        binding.name = new_name.to_string();
        self.apply_binding_command(build_update_command(binding));
        self.propagate_bindings();
        self.refresh_models();
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
        let Some(b) = self.bindings().into_iter().find(|b| b.id == id) else {
            return;
        };
        let cmd = if !edit_name.is_empty() {
            // Edit: replace the endpoint in place, keeping its name.
            let ep = if is_input {
                build_input_endpoint(edit_name, device_id, channels, parsed_mode)
            } else {
                build_output_endpoint(edit_name, device_id, channels, parsed_mode)
            };
            build_update_replacing_endpoint(b, edit_name, ep, is_input)
        } else {
            let existing = if is_input {
                b.inputs.len()
            } else {
                b.outputs.len()
            };
            let name = next_endpoint_name(existing, is_input);
            if is_input {
                let ep = build_input_endpoint(&name, device_id, channels, parsed_mode);
                build_update_with_input_endpoint(b, ep)
            } else {
                let ep = build_output_endpoint(&name, device_id, channels, parsed_mode);
                build_update_with_output_endpoint(b, ep)
            }
        };
        self.apply_binding_command(cmd);
        self.models.channels.set_vec(Vec::new());
        self.propagate_bindings();
        self.refresh_models();
    }

    /// Seed the channel model + prefill props for editing an existing endpoint,
    /// and return the (device_index, mode_index) the form should preselect.
    fn edit_endpoint(&self, id: &str, ep_name: &str, is_input: bool) -> (i32, i32) {
        let devices = if is_input {
            self.input_devices.borrow()
        } else {
            self.output_devices.borrow()
        };
        let bindings = self.bindings();
        let Some(binding) = bindings.iter().find(|b| b.id == id) else {
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
        let Some(b) = self.bindings().into_iter().find(|b| b.id == id) else {
            return;
        };
        self.apply_binding_command(build_update_removing_endpoint(b, ep_name, is_input));
        self.propagate_bindings();
        self.refresh_models();
    }
}

fn make_ctx(
    ps: &Rc<RefCell<Option<ProjectSession>>>,
    cfg: &Rc<RefCell<AppConfig>>,
    models: &Rc<BindingModels>,
    input_devices: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_devices: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
) -> Rc<WireCtx> {
    Rc::new(WireCtx {
        ps: Rc::clone(ps),
        cfg: Rc::clone(cfg),
        models: Rc::clone(models),
        input_devices: Rc::clone(input_devices),
        output_devices: Rc::clone(output_devices),
    })
}

pub(super) fn install_window_callbacks(
    window: &AppWindow,
    ps: &Rc<RefCell<Option<ProjectSession>>>,
    cfg: &Rc<RefCell<AppConfig>>,
    models: &Rc<BindingModels>,
    input_devices: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_devices: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
) {
    let ctx = make_ctx(ps, cfg, models, input_devices, output_devices);

    let c = ctx.clone();
    crate::SettingsBridge::get(window)
        .on_create_io_binding(move |name| c.create_binding(name.as_str()));
    let c = ctx.clone();
    crate::SettingsBridge::get(window)
        .on_delete_io_binding(move |id| c.delete_binding(id.as_str()));
    let c = ctx.clone();
    crate::SettingsBridge::get(window)
        .on_rename_io_binding(move |id, n| c.rename_binding(id.as_str(), n.as_str()));
    let c = ctx.clone();
    crate::SettingsBridge::get(window).on_endpoint_device_changed(move |_id, is_input, dev| {
        c.device_changed(is_input, dev.as_str())
    });
    let c = ctx.clone();
    crate::SettingsBridge::get(window).on_toggle_endpoint_channel(move |idx, sel, mode| {
        c.toggle_channel(idx, sel, mode.as_str())
    });
    let c = ctx.clone();
    crate::SettingsBridge::get(window).on_add_input_endpoint(move |id, dev, mode, en| {
        c.add_endpoint(id.as_str(), dev.as_str(), mode.as_str(), true, en.as_str())
    });
    let c = ctx.clone();
    crate::SettingsBridge::get(window).on_add_output_endpoint(move |id, dev, mode, en| {
        c.add_endpoint(id.as_str(), dev.as_str(), mode.as_str(), false, en.as_str())
    });
    let c = ctx.clone();
    crate::SettingsBridge::get(window)
        .on_remove_endpoint(move |id, en, inp| c.remove_endpoint(id.as_str(), en.as_str(), inp));
    let c = ctx.clone();
    let weak = window.as_weak();
    crate::SettingsBridge::get(window).on_edit_endpoint(move |id, en, inp| {
        let (dev_idx, mode_idx) = c.edit_endpoint(id.as_str(), en.as_str(), inp);
        if let Some(w) = weak.upgrade() {
            crate::SettingsBridge::get(&w).set_io_edit_prefill_device_index(dev_idx);
            crate::SettingsBridge::get(&w).set_io_edit_prefill_mode_index(mode_idx);
        }
    });
}

pub(super) fn install_psw_callbacks(
    psw: &ProjectSettingsWindow,
    ps: &Rc<RefCell<Option<ProjectSession>>>,
    cfg: &Rc<RefCell<AppConfig>>,
    models: &Rc<BindingModels>,
    input_devices: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_devices: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
) {
    let ctx = make_ctx(ps, cfg, models, input_devices, output_devices);

    let c = ctx.clone();
    crate::SettingsBridge::get(psw)
        .on_create_io_binding(move |name| c.create_binding(name.as_str()));
    let c = ctx.clone();
    crate::SettingsBridge::get(psw).on_delete_io_binding(move |id| c.delete_binding(id.as_str()));
    let c = ctx.clone();
    crate::SettingsBridge::get(psw)
        .on_rename_io_binding(move |id, n| c.rename_binding(id.as_str(), n.as_str()));
    let c = ctx.clone();
    crate::SettingsBridge::get(psw).on_endpoint_device_changed(move |_id, is_input, dev| {
        c.device_changed(is_input, dev.as_str())
    });
    let c = ctx.clone();
    crate::SettingsBridge::get(psw).on_toggle_endpoint_channel(move |idx, sel, mode| {
        c.toggle_channel(idx, sel, mode.as_str())
    });
    let c = ctx.clone();
    crate::SettingsBridge::get(psw).on_add_input_endpoint(move |id, dev, mode, en| {
        c.add_endpoint(id.as_str(), dev.as_str(), mode.as_str(), true, en.as_str())
    });
    let c = ctx.clone();
    crate::SettingsBridge::get(psw).on_add_output_endpoint(move |id, dev, mode, en| {
        c.add_endpoint(id.as_str(), dev.as_str(), mode.as_str(), false, en.as_str())
    });
    let c = ctx.clone();
    crate::SettingsBridge::get(psw)
        .on_remove_endpoint(move |id, en, inp| c.remove_endpoint(id.as_str(), en.as_str(), inp));
    let c = ctx.clone();
    let weak = psw.as_weak();
    crate::SettingsBridge::get(psw).on_edit_endpoint(move |id, en, inp| {
        let (dev_idx, mode_idx) = c.edit_endpoint(id.as_str(), en.as_str(), inp);
        if let Some(w) = weak.upgrade() {
            crate::SettingsBridge::get(&w).set_io_edit_prefill_device_index(dev_idx);
            crate::SettingsBridge::get(&w).set_io_edit_prefill_mode_index(mode_idx);
        }
    });
}
