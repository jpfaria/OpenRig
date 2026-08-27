//! Responsibility: edits the device rows the settings screen shows.

use crate::DeviceSelectionItem;
use domain::AudioDeviceDescriptor;
use infra_filesystem::GuiAudioDeviceSettings;
use project::device::DeviceSettings;
use slint::{Model, VecModel};
use std::rc::Rc;

use crate::device_settings_resolve::default_device_settings;

pub(crate) fn build_project_device_rows(
    input_devices: &[AudioDeviceDescriptor],
    output_devices: &[AudioDeviceDescriptor],
    device_settings: &[DeviceSettings],
) -> Vec<DeviceSelectionItem> {
    let mut rows: Vec<DeviceSelectionItem> = Vec::new();
    for device in input_devices.iter().chain(output_devices.iter()) {
        if rows.iter().any(|row| {
            row.device_id.as_str() == device.id.as_str()
                || row.name.as_str() == device.name.as_str()
        }) {
            continue;
        }
        let config = device_settings
            .iter()
            .find(|setting| setting.device_id.0 == device.id)
            .map(|setting| GuiAudioDeviceSettings {
                device_id: setting.device_id.0.clone(),
                name: device.name.clone(),
                sample_rate: setting.sample_rate,
                buffer_size_frames: setting.buffer_size_frames,
                bit_depth: setting.bit_depth,
                #[cfg(target_os = "linux")]
                realtime: setting.realtime,
                #[cfg(target_os = "linux")]
                rt_priority: setting.rt_priority,
                #[cfg(target_os = "linux")]
                nperiods: setting.nperiods,
            })
            .unwrap_or_else(|| default_device_settings(device.id.clone(), device.name.clone()));
        rows.push(DeviceSelectionItem {
            device_id: config.device_id.into(),
            name: config.name.into(),
            selected: device_settings
                .iter()
                .any(|setting| setting.device_id.0 == device.id),
            sample_rate_text: config.sample_rate.to_string().into(),
            buffer_size_text: config.buffer_size_frames.to_string().into(),
            bit_depth_text: config.bit_depth.to_string().into(),
        });
    }
    rows
}

pub(crate) fn toggle_device_row(
    model: &Rc<VecModel<DeviceSelectionItem>>,
    index: usize,
    selected: bool,
) {
    if let Some(mut row) = model.row_data(index) {
        row.selected = selected;
        model.set_row_data(index, row);
    }
}

pub(crate) fn update_device_sample_rate(
    model: &Rc<VecModel<DeviceSelectionItem>>,
    index: usize,
    value: slint::SharedString,
) {
    if let Some(mut row) = model.row_data(index) {
        row.sample_rate_text = value;
        model.set_row_data(index, row);
    }
}

pub(crate) fn update_device_buffer_size(
    model: &Rc<VecModel<DeviceSelectionItem>>,
    index: usize,
    value: slint::SharedString,
) {
    if let Some(mut row) = model.row_data(index) {
        row.buffer_size_text = value;
        model.set_row_data(index, row);
    }
}

pub(crate) fn update_device_bit_depth(
    model: &Rc<VecModel<DeviceSelectionItem>>,
    index: usize,
    value: slint::SharedString,
) {
    if let Some(mut row) = model.row_data(index) {
        row.bit_depth_text = value;
        model.set_row_data(index, row);
    }
}
