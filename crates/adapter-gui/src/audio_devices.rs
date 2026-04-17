use anyhow::{anyhow, Result};
use infra_cpal::{AudioDeviceDescriptor, list_input_device_descriptors, list_output_device_descriptors};
use infra_filesystem::GuiAudioDeviceSettings;
use project::device::DeviceSettings;
use project::project::Project;
use slint::{SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use crate::{
    ChannelOptionItem, DeviceSelectionItem,
    DEFAULT_SAMPLE_RATE, DEFAULT_BUFFER_SIZE_FRAMES, DEFAULT_BIT_DEPTH,
    SUPPORTED_SAMPLE_RATES, SUPPORTED_BUFFER_SIZES, SUPPORTED_BIT_DEPTHS,
};
use crate::state::{ChainDraft, InputGroupDraft, InsertDraft, OutputGroupDraft};

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
    draft: &ChainDraft,
    project: &Project,
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
    let device_id = &device.id;
    // A disabled chain can claim any channel — it is not running so there is no
    // real conflict. Only show conflicts when the chain being edited is enabled.
    let editing_chain_enabled = draft.editing_index
        .and_then(|i| project.chains.get(i))
        .map(|c| c.enabled)
        .unwrap_or(true); // new chains (no editing_index) behave as enabled for conflict purposes
    let used_channels: Vec<usize> = if !editing_chain_enabled {
        Vec::new()
    } else {
        project
            .chains
            .iter()
            .enumerate()
            .filter(|(index, chain)| {
                chain.enabled && draft.editing_index != Some(*index)
            })
            .flat_map(|(_, chain)| {
                chain.input_blocks().into_iter()
                    .flat_map(|(_, inp)| inp.entries.iter())
                    .filter(|entry| entry.device_id.0 == *device_id)
                    .flat_map(|entry| entry.channels.iter().copied().collect::<Vec<_>>())
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    (0..device.channels)
        .map(|channel| ChannelOptionItem {
            index: channel as i32,
            label: format!("Canal {}", channel + 1).into(),
            selected: input_group.channels.contains(&channel),
            available: !used_channels.contains(&channel),
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
            label: format!("Canal {}", channel + 1).into(),
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
            label: format!("Canal {}", channel + 1).into(),
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
            label: format!("Canal {}", channel + 1).into(),
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
