//! Responsibility: shapes the device list for the picker to bind to.

use crate::DeviceSelectionItem;
use domain::AudioDeviceDescriptor;
use infra_filesystem::GuiAudioDeviceSettings;
use slint::{Model, VecModel};
use std::rc::Rc;

use crate::device_settings_resolve::{default_device_settings, normalize_device_settings};

/// Build the `DeviceSelectionItem` rows shown in the project Settings panel.
///
/// Each descriptor is matched against the user's saved per-device config —
/// when present it's normalized (sample rate / buffer / bit-depth clamped to
/// supported values), otherwise it falls back to defaults. `selected = true`
/// here means "currently visible in the descriptor list"; the caller pairs
/// this with [`mark_unselected_devices`] to flip rows the user has explicitly
/// turned off in `gui-settings.yaml`.
pub(crate) fn build_device_selection_items(
    descriptors: &[AudioDeviceDescriptor],
    saved: &[GuiAudioDeviceSettings],
) -> Vec<DeviceSelectionItem> {
    descriptors
        .iter()
        .map(|device| {
            let device_id = device.id.clone();
            let name = device.name.clone();
            let config = saved
                .iter()
                .find(|s| s.device_id == device_id)
                .cloned()
                .map(normalize_device_settings)
                .unwrap_or_else(|| default_device_settings(device_id.clone(), name.clone()));
            DeviceSelectionItem {
                device_id: config.device_id.into(),
                name: config.name.into(),
                selected: true,
                sample_rate_text: config.sample_rate.to_string().into(),
                buffer_size_text: config.buffer_size_frames.to_string().into(),
                bit_depth_text: config.bit_depth.to_string().into(),
            }
        })
        .collect()
}

pub(crate) fn mark_unselected_devices(
    model: &Rc<VecModel<DeviceSelectionItem>>,
    selected_devices: &[GuiAudioDeviceSettings],
) {
    for index in 0..model.row_count() {
        let Some(mut row) = model.row_data(index) else {
            continue;
        };
        row.selected = selected_devices
            .iter()
            .any(|saved| saved.device_id == row.device_id.as_str());
        model.set_row_data(index, row);
    }
}
