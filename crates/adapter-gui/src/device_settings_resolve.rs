//! Responsibility: resolves the settings one device is opened with.

use crate::{
    DeviceSelectionItem, DEFAULT_BIT_DEPTH, DEFAULT_BUFFER_SIZE_FRAMES, DEFAULT_SAMPLE_RATE,
    SUPPORTED_BIT_DEPTHS, SUPPORTED_BUFFER_SIZES, SUPPORTED_SAMPLE_RATES,
};
use anyhow::{anyhow, Result};
use infra_filesystem::GuiAudioDeviceSettings;
use slint::{Model, VecModel};
use std::rc::Rc;

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

pub(crate) fn normalize_device_settings(
    mut settings: GuiAudioDeviceSettings,
) -> GuiAudioDeviceSettings {
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

fn parse_positive_u32(value: &str, field: &str) -> Result<u32> {
    value
        .trim()
        .parse::<u32>()
        .map_err(|_| anyhow!("'{}' inválido: '{}'", field, value))
}
