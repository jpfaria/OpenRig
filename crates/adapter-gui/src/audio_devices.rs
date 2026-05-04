use anyhow::{anyhow, Result};
use infra_cpal::{AudioDeviceDescriptor, list_input_device_descriptors, list_output_device_descriptors};
use infra_filesystem::GuiAudioDeviceSettings;
use project::device::DeviceSettings;
use slint::{Model, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use crate::{
    ChannelOptionItem, DeviceSelectionItem,
    DEFAULT_SAMPLE_RATE, DEFAULT_BUFFER_SIZE_FRAMES, DEFAULT_BIT_DEPTH,
    SUPPORTED_SAMPLE_RATES, SUPPORTED_BUFFER_SIZES, SUPPORTED_BIT_DEPTHS,
};
use crate::state::{InputGroupDraft, InsertDraft, OutputGroupDraft};

pub(crate) fn refresh_input_devices(
    device_options_model: &Rc<VecModel<SharedString>>,
) -> Vec<AudioDeviceDescriptor> {
    let devices = list_input_device_descriptors().unwrap_or_default();
    let names: Vec<SharedString> = devices
        .iter()
        .map(|d| SharedString::from(d.name.as_str()))
        .collect();
    device_options_model.set_vec(names);
    devices
}

pub(crate) fn refresh_output_devices(
    device_options_model: &Rc<VecModel<SharedString>>,
) -> Vec<AudioDeviceDescriptor> {
    let devices = list_output_device_descriptors().unwrap_or_default();
    let names: Vec<SharedString> = devices
        .iter()
        .map(|d| SharedString::from(d.name.as_str()))
        .collect();
    device_options_model.set_vec(names);
    devices
}

pub(crate) fn ensure_devices_loaded(
    input: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
) {
    if input.borrow().is_empty() {
        *input.borrow_mut() = list_input_device_descriptors().unwrap_or_default();
    }
    if output.borrow().is_empty() {
        *output.borrow_mut() = list_output_device_descriptors().unwrap_or_default();
    }
}

pub(crate) fn selected_device_index(devices: &[AudioDeviceDescriptor], selected_id: Option<&str>) -> i32 {
    let exact = selected_id
        .and_then(|sid| devices.iter().position(|device| device.id == sid))
        .map(|index| index as i32);
    if let Some(idx) = exact {
        return idx;
    }
    // Fallback: when the saved device_id doesn't match any listed device
    // (e.g., JACK id "jack:system" vs ALSA ids when JACK is not running),
    // auto-select the only device if there is exactly one.
    if selected_id.is_some() && devices.len() == 1 {
        return 0;
    }
    -1
}

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

pub(crate) fn build_input_channel_items(
    input_group: &InputGroupDraft,
    input_devices: &[AudioDeviceDescriptor],
) -> Vec<ChannelOptionItem> {
    let Some(device_id) = input_group.device_id.as_ref() else {
        return Vec::new();
    };
    // Try exact match first, then fallback to single device
    let device = input_devices.iter().find(|d| &d.id == device_id)
        .or_else(|| if input_devices.len() == 1 { input_devices.first() } else { None });
    let Some(device) = device else {
        return Vec::new();
    };
    // Input channels are freely shareable across chains — backend fan-out
    // (cpal/JACK opens the device once and dispatches to N parallel
    // runtimes). #317: do NOT mark a channel unavailable just because
    // another chain consumes it.
    (0..device.channels)
        .map(|channel| ChannelOptionItem {
            index: channel as i32,
            label: rust_i18n::t!("label-channel-numbered", n = channel + 1).to_string().into(),
            selected: input_group.channels.contains(&channel),
            available: true,
        })
        .collect()
}

pub(crate) fn build_output_channel_items(
    output_group: &OutputGroupDraft,
    output_devices: &[AudioDeviceDescriptor],
) -> Vec<ChannelOptionItem> {
    let Some(device_id) = output_group.device_id.as_ref() else {
        return Vec::new();
    };
    let device = output_devices.iter().find(|d| &d.id == device_id)
        .or_else(|| if output_devices.len() == 1 { output_devices.first() } else { None });
    let Some(device) = device else {
        return Vec::new();
    };
    (0..device.channels)
        .map(|channel| ChannelOptionItem {
            index: channel as i32,
            label: rust_i18n::t!("label-channel-numbered", n = channel + 1).to_string().into(),
            selected: output_group.channels.contains(&channel),
            available: true,
        })
        .collect()
}

pub(crate) fn replace_channel_options(model: &Rc<VecModel<ChannelOptionItem>>, items: Vec<ChannelOptionItem>) {
    model.set_vec(items);
}

pub(crate) fn build_insert_send_channel_items(
    draft: &InsertDraft,
    output_devices: &[AudioDeviceDescriptor],
) -> Vec<ChannelOptionItem> {
    let Some(device_id) = draft.send_device_id.as_ref() else {
        return Vec::new();
    };
    let Some(device) = output_devices.iter().find(|d| &d.id == device_id) else {
        return Vec::new();
    };
    (0..device.channels)
        .map(|channel| ChannelOptionItem {
            index: channel as i32,
            label: rust_i18n::t!("label-channel-numbered", n = channel + 1).to_string().into(),
            selected: draft.send_channels.contains(&channel),
            available: true,
        })
        .collect()
}

pub(crate) fn build_insert_return_channel_items(
    draft: &InsertDraft,
    input_devices: &[AudioDeviceDescriptor],
) -> Vec<ChannelOptionItem> {
    let Some(device_id) = draft.return_device_id.as_ref() else {
        return Vec::new();
    };
    let Some(device) = input_devices.iter().find(|d| &d.id == device_id) else {
        return Vec::new();
    };
    (0..device.channels)
        .map(|channel| ChannelOptionItem {
            index: channel as i32,
            label: rust_i18n::t!("label-channel-numbered", n = channel + 1).to_string().into(),
            selected: draft.return_channels.contains(&channel),
            available: true,
        })
        .collect()
}

pub(crate) fn toggle_device_row(model: &Rc<VecModel<DeviceSelectionItem>>, index: usize, selected: bool) {
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

pub(crate) fn selected_device_settings(
    model: &Rc<VecModel<DeviceSelectionItem>>,
    device_kind: &str,
) -> Result<Vec<GuiAudioDeviceSettings>> {
    (0..model.row_count())
        .filter_map(|index| model.row_data(index))
        .filter(|row| row.selected)
        .map(|row| {
            Ok(GuiAudioDeviceSettings {
                device_id: row.device_id.to_string(),
                name: row.name.to_string(),
                sample_rate: parse_positive_u32(
                    row.sample_rate_text.as_str(),
                    &format!("{}_sample_rate '{}'", device_kind, row.name),
                )?,
                buffer_size_frames: parse_positive_u32(
                    row.buffer_size_text.as_str(),
                    &format!("{}_buffer_size_frames '{}'", device_kind, row.name),
                )?,
                bit_depth: parse_positive_u32(
                    row.bit_depth_text.as_str(),
                    &format!("{}_bit_depth '{}'", device_kind, row.name),
                )?,
                // Low-latency defaults — JACK tuning isn't exposed in the UI;
                // users get RT priority + nperiods=3 out of the box (nperiods=2
                // triggered ALSA Broken pipe on Q26 USB audio + RK3588, so we
                // stay on nperiods=3 until per-device profiles land). Override
                // by editing gui-settings.yaml directly if needed.
                #[cfg(target_os = "linux")]
                realtime: true,
                #[cfg(target_os = "linux")]
                rt_priority: 70,
                #[cfg(target_os = "linux")]
                nperiods: 3,
            })
        })
        .collect()
}

pub(crate) fn default_device_settings(device_id: String, name: String) -> GuiAudioDeviceSettings {
    GuiAudioDeviceSettings {
        device_id,
        name,
        sample_rate: DEFAULT_SAMPLE_RATE,
        buffer_size_frames: DEFAULT_BUFFER_SIZE_FRAMES,
        bit_depth: DEFAULT_BIT_DEPTH,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }
}

pub(crate) fn normalize_device_settings(mut settings: GuiAudioDeviceSettings) -> GuiAudioDeviceSettings {
    if !SUPPORTED_SAMPLE_RATES.contains(&settings.sample_rate) {
        settings.sample_rate = DEFAULT_SAMPLE_RATE;
    }
    if !SUPPORTED_BUFFER_SIZES.contains(&settings.buffer_size_frames) {
        settings.buffer_size_frames = DEFAULT_BUFFER_SIZE_FRAMES;
    }
    if !SUPPORTED_BIT_DEPTHS.contains(&settings.bit_depth) {
        settings.bit_depth = DEFAULT_BIT_DEPTH;
    }
    settings
}

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

fn parse_positive_u32(value: &str, field: &str) -> Result<u32> {
    value
        .trim()
        .parse::<u32>()
        .map_err(|_| anyhow!("'{}' inválido: '{}'", field, value))
}
