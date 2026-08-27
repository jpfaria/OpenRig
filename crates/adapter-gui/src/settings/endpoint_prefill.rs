//! Responsibility: fills the endpoint editor with what is already stored.

use crate::settings::endpoint_channels::channel_items_for_device;
use crate::ChannelOptionItem;
use domain::io_binding::{ChannelMode, IoBinding};
use domain::AudioDeviceDescriptor;

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
