//! Pure endpoint helpers for the System / I/O bindings editor (#716).
//!
//! These functions translate the structured picker inputs (device id +
//! 0-based channel indices + mode) into a domain `IoEndpoint` and the
//! `Command::UpdateIoBinding` that the dispatcher consumes. They are
//! deliberately free of any `AppWindow` so the wiring is testable headless
//! (LAW 1).
//!
//! Channel options for a chosen device are derived ONLY from the enumerated
//! `AudioDeviceDescriptor.channels` count — there is no hardcoded channel
//! count or default device. An unknown device id yields no channels.

use application::command::Command;
use domain::ids::DeviceId;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use infra_cpal::AudioDeviceDescriptor;

use crate::ChannelOptionItem;

/// Parse the snake_case wire token into a `ChannelMode`. Unknown tokens fall
/// back to `Mono` (the domain default); the picker only ever emits the three
/// valid tokens, so this is a defensive default, not a hardcoded device value.
pub(crate) fn channel_mode_from_str(s: &str) -> ChannelMode {
    match s {
        "stereo" => ChannelMode::Stereo,
        "dual_mono" => ChannelMode::DualMono,
        _ => ChannelMode::Mono,
    }
}

/// Build the per-channel checkbox options for `device_id`, derived from the
/// device's reported channel count. `selected` marks which 0-based indices are
/// currently chosen. An unknown device id yields an empty list (no fallback to
/// a default device or a hardcoded channel count).
pub(crate) fn channel_items_for_device(
    device_id: &str,
    devices: &[AudioDeviceDescriptor],
    selected: &[usize],
) -> Vec<ChannelOptionItem> {
    let Some(device) = devices.iter().find(|d| d.id == device_id) else {
        return Vec::new();
    };
    (0..device.channels)
        .map(|channel| ChannelOptionItem {
            index: channel as i32,
            label: rust_i18n::t!("label-channel-numbered", n = channel + 1)
                .to_string()
                .into(),
            selected: selected.contains(&channel),
            available: true,
        })
        .collect()
}

/// Apply a channel toggle honouring the mode's selection rule. Mono is a radio
/// group (exactly one channel): selecting a channel deselects every other.
/// Stereo and DualMono are checkbox sets where multiple channels may be
/// selected at once. Returns the updated option list (pure — no model mutation).
pub(crate) fn apply_channel_toggle(
    items: &[ChannelOptionItem],
    index: i32,
    selected: bool,
    mode: ChannelMode,
) -> Vec<ChannelOptionItem> {
    let single_select = mode == ChannelMode::Mono;
    items
        .iter()
        .map(|item| {
            let mut next = item.clone();
            if item.index == index {
                next.selected = selected;
            } else if single_select && selected {
                // Radio behaviour: choosing one clears the rest.
                next.selected = false;
            }
            next
        })
        .collect()
}

/// Build an input `IoEndpoint` from the structured picker inputs.
pub(crate) fn build_input_endpoint(
    name: &str,
    device_id: &str,
    channels: Vec<usize>,
    mode: ChannelMode,
) -> IoEndpoint {
    IoEndpoint {
        name: name.to_string(),
        device_id: DeviceId(device_id.to_string()),
        mode,
        channels,
    }
}

/// Build an output `IoEndpoint` from the structured picker inputs. Symmetric
/// to [`build_input_endpoint`]; the output picker constrains `mode` to
/// mono/stereo at the UI layer.
pub(crate) fn build_output_endpoint(
    name: &str,
    device_id: &str,
    channels: Vec<usize>,
    mode: ChannelMode,
) -> IoEndpoint {
    IoEndpoint {
        name: name.to_string(),
        device_id: DeviceId(device_id.to_string()),
        mode,
        channels,
    }
}

/// Append `new_ep` to `binding.inputs` and wrap it in `Command::UpdateIoBinding`.
pub(crate) fn build_update_with_input_endpoint(
    mut binding: IoBinding,
    new_ep: IoEndpoint,
) -> Command {
    binding.inputs.push(new_ep);
    Command::UpdateIoBinding { binding }
}

/// Append `new_ep` to `binding.outputs` and wrap it in `Command::UpdateIoBinding`.
pub(crate) fn build_update_with_output_endpoint(
    mut binding: IoBinding,
    new_ep: IoEndpoint,
) -> Command {
    binding.outputs.push(new_ep);
    Command::UpdateIoBinding { binding }
}

/// Prefill data for re-opening the add-form on an existing endpoint (edit).
pub(crate) struct EndpointPrefill {
    /// Index of the endpoint's device in the side's device list, or -1 if the
    /// device is no longer enumerated.
    pub device_index: i32,
    /// The endpoint's channel layout, to preselect the mode segment.
    pub mode: ChannelMode,
    /// Channel options for the device with the endpoint's channels selected.
    pub channel_items: Vec<ChannelOptionItem>,
}

/// Resolve the prefill for editing `ep_name` on `binding`: find the endpoint,
/// locate its device in `devices`, and rebuild the channel options with the
/// endpoint's channels pre-selected. Returns `None` if the endpoint is absent.
pub(crate) fn endpoint_prefill(
    binding: &IoBinding,
    ep_name: &str,
    is_input: bool,
    devices: &[AudioDeviceDescriptor],
) -> Option<EndpointPrefill> {
    let list = if is_input {
        &binding.inputs
    } else {
        &binding.outputs
    };
    let ep = list.iter().find(|e| e.name == ep_name)?;
    let device_index = devices
        .iter()
        .position(|d| d.id == ep.device_id.0)
        .map(|i| i as i32)
        .unwrap_or(-1);
    let channel_items = channel_items_for_device(&ep.device_id.0, devices, &ep.channels);
    Some(EndpointPrefill {
        device_index,
        mode: ep.mode,
        channel_items,
    })
}

/// Replace the endpoint named `old_name` on the matching side with `new_ep`,
/// preserving the position of every other endpoint, and wrap the result in
/// `Command::UpdateIoBinding`. Used by the edit-endpoint save path (Bug 3).
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
    Command::UpdateIoBinding { binding }
}

/// Drop the endpoint named `ep_name` from the matching side (input vs output)
/// and wrap the result in `Command::UpdateIoBinding`.
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
    Command::UpdateIoBinding { binding }
}

/// Snake_case wire token for a `ChannelMode`, for the read-back display models.
pub(crate) fn mode_label(mode: ChannelMode) -> &'static str {
    match mode {
        ChannelMode::Mono => "mono",
        ChannelMode::Stereo => "stereo",
        ChannelMode::DualMono => "dual_mono",
    }
}

/// Sequential default endpoint name ("In N" / "Out N") so a structured add
/// always yields a labelled, removable endpoint without free text.
pub(crate) fn next_endpoint_name(existing: usize, is_input: bool) -> String {
    let prefix = if is_input { "In" } else { "Out" };
    format!("{prefix} {}", existing + 1)
}
