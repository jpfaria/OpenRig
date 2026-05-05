use domain::ids::DeviceId;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceSettings {
    pub device_id: DeviceId,
    pub sample_rate: u32,
    pub buffer_size_frames: u32,
    #[serde(default = "default_bit_depth")]
    pub bit_depth: u32,
    // JACK-only tuning (Linux). Consumed exclusively by infra-cpal's jack
    // supervisor; absent from the struct and YAML on macOS/Windows where
    // cpal backends (CoreAudio/WASAPI) don't honour these knobs.
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

#[cfg(test)]
#[path = "device_tests.rs"]
mod tests;
