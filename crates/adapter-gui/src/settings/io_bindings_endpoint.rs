//! Responsibility: builds the endpoint rows of the bindings section.
//! Pure endpoint helpers for the System / I/O bindings editor (#716).
//!
//! These functions translate the structured picker inputs (device id +
//! 0-based channel indices + mode) into a domain `IoEndpoint` and the
//! `IoBindingCommand::UpdateIoBinding` that the dispatcher consumes. They are
//! deliberately free of any `AppWindow` so the wiring is testable headless
//! (LAW 1).
//!
//! Channel options for a chosen device are derived ONLY from the enumerated
//! `AudioDeviceDescriptor.channels` count — there is no hardcoded channel
//! count or default device. An unknown device id yields no channels.

use application::command::{Command, IoBindingCommand};
use domain::io_binding::{IoBinding, IoEndpoint};

pub(crate) use crate::settings::endpoint_build::{build_input_endpoint, build_output_endpoint};
pub(crate) use crate::settings::endpoint_channels::{
    apply_channel_toggle, channel_items_for_device, channel_mode_from_str, mode_label,
    next_endpoint_name,
};
pub(crate) use crate::settings::endpoint_prefill::endpoint_prefill;

/// Wrap `binding` as-is in `IoBindingCommand::UpdateIoBinding`.
pub(crate) fn build_update_command(binding: IoBinding) -> Command {
    Command::IoBinding(IoBindingCommand::UpdateIoBinding { binding })
}

/// Append `new_ep` to `binding.inputs` and wrap it in `IoBindingCommand::UpdateIoBinding`.
pub(crate) fn build_update_with_input_endpoint(
    mut binding: IoBinding,
    new_ep: IoEndpoint,
) -> Command {
    binding.inputs.push(new_ep);
    Command::IoBinding(IoBindingCommand::UpdateIoBinding { binding })
}

/// Append `new_ep` to `binding.outputs` and wrap it in `IoBindingCommand::UpdateIoBinding`.
pub(crate) fn build_update_with_output_endpoint(
    mut binding: IoBinding,
    new_ep: IoEndpoint,
) -> Command {
    binding.outputs.push(new_ep);
    Command::IoBinding(IoBindingCommand::UpdateIoBinding { binding })
}

/// Replace the endpoint named `old_name` on the matching side with `new_ep`,
/// preserving the position of every other endpoint, and wrap the result in
/// `IoBindingCommand::UpdateIoBinding`. Used by the edit-endpoint save path (Bug 3).
pub(crate) fn build_update_replacing_endpoint(
    mut binding: IoBinding,
    old_name: &str,
    new_ep: IoEndpoint,
    is_input: bool,
) -> Command {
    let list = if is_input {
        &mut binding.inputs
    } else {
        &mut binding.outputs
    };
    if let Some(slot) = list.iter_mut().find(|e| e.name == old_name) {
        *slot = new_ep;
    } else {
        list.push(new_ep);
    }
    Command::IoBinding(IoBindingCommand::UpdateIoBinding { binding })
}

/// Drop the endpoint named `ep_name` from the matching side (input vs output)
/// and wrap the result in `IoBindingCommand::UpdateIoBinding`.
pub(crate) fn build_update_removing_endpoint(
    mut binding: IoBinding,
    ep_name: &str,
    is_input: bool,
) -> Command {
    if is_input {
        binding.inputs.retain(|e| e.name != ep_name);
    } else {
        binding.outputs.retain(|e| e.name != ep_name);
    }
    Command::IoBinding(IoBindingCommand::UpdateIoBinding { binding })
}
