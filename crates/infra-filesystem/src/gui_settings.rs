//! Responsibility: describes the audio preferences a machine keeps for its GUI.
//!
//! Split out of `lib.rs` (#873). Includes the conversion from the historical
//! `gui-settings.yaml` shape, which is still read once on first load before
//! the file is deleted.

use serde::{Deserialize, Serialize};

use crate::midi_device::MidiDeviceSelection;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GuiAudioDeviceSettings {
    pub device_id: String,
    pub name: String,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    #[serde(default = "default_buffer_size_frames")]
    pub buffer_size_frames: u32,
    #[serde(default = "default_bit_depth")]
    pub bit_depth: u32,
    // Linux JACK tuning — only present on Linux builds. cpal backends on
    // macOS (CoreAudio) and Windows (WASAPI/ASIO) don't honour realtime
    // priority or ALSA nperiods, so the fields don't exist there and the
    // YAML stays clean.
    #[cfg(target_os = "linux")]
    #[serde(default = "default_realtime")]
    pub realtime: bool,
    #[cfg(target_os = "linux")]
    #[serde(default = "default_rt_priority")]
    pub rt_priority: u8,
    #[cfg(target_os = "linux")]
    #[serde(default = "default_nperiods")]
    pub nperiods: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GuiSystemSettings {
    #[serde(default)]
    pub input_devices: Vec<GuiAudioDeviceSettings>,
    #[serde(default)]
    pub output_devices: Vec<GuiAudioDeviceSettings>,
    // Renamed from GuiAudioSettings (#513) to reflect that it holds every
    // per-machine GUI preference, not just audio.
    // None / "auto" follows the OS locale; "pt-BR" / "en-US" override it.
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub midi_devices: Vec<MidiDeviceSelection>,
}

impl GuiSystemSettings {
    pub fn is_complete(&self) -> bool {
        !self.input_devices.is_empty() && !self.output_devices.is_empty()
    }
}

fn default_sample_rate() -> u32 {
    48_000
}

fn default_buffer_size_frames() -> u32 {
    256
}

fn default_bit_depth() -> u32 {
    32
}

#[cfg(target_os = "linux")]
fn default_realtime() -> bool {
    true
}

#[cfg(target_os = "linux")]
fn default_rt_priority() -> u8 {
    70
}

#[cfg(target_os = "linux")]
fn default_nperiods() -> u32 {
    3
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) struct LegacyGuiAudioSettings {
    #[serde(default)]
    pub(crate) input_device_names: Vec<String>,
    #[serde(default)]
    pub(crate) output_device_names: Vec<String>,
    #[serde(default = "default_sample_rate")]
    pub(crate) sample_rate: u32,
    #[serde(default = "default_buffer_size_frames")]
    pub(crate) buffer_size_frames: u32,
}

impl From<LegacyGuiAudioSettings> for GuiSystemSettings {
    fn from(value: LegacyGuiAudioSettings) -> Self {
        let input_devices = value
            .input_device_names
            .into_iter()
            .map(|name| GuiAudioDeviceSettings {
                device_id: String::new(),
                name,
                sample_rate: value.sample_rate,
                buffer_size_frames: value.buffer_size_frames,
                bit_depth: default_bit_depth(),
                #[cfg(target_os = "linux")]
                realtime: default_realtime(),
                #[cfg(target_os = "linux")]
                rt_priority: default_rt_priority(),
                #[cfg(target_os = "linux")]
                nperiods: default_nperiods(),
            })
            .collect();
        let output_devices = value
            .output_device_names
            .into_iter()
            .map(|name| GuiAudioDeviceSettings {
                device_id: String::new(),
                name,
                sample_rate: value.sample_rate,
                buffer_size_frames: value.buffer_size_frames,
                bit_depth: default_bit_depth(),
                #[cfg(target_os = "linux")]
                realtime: default_realtime(),
                #[cfg(target_os = "linux")]
                rt_priority: default_rt_priority(),
                #[cfg(target_os = "linux")]
                nperiods: default_nperiods(),
            })
            .collect();
        Self {
            input_devices,
            output_devices,
            language: None,
            midi_devices: vec![],
        }
    }
}
