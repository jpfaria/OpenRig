//! Responsibility: maps the device settings document onto the runtime device settings.
//!
//! Split out of `lib.rs` (#873).

use serde::{Deserialize, Serialize};

use domain::ids::DeviceId;
use project::device::DeviceSettings;

fn default_yaml_bit_depth() -> u32 {
    32
}

#[cfg(target_os = "linux")]
fn default_yaml_realtime() -> bool {
    true
}

#[cfg(target_os = "linux")]
fn default_yaml_rt_priority() -> u8 {
    70
}

#[cfg(target_os = "linux")]
fn default_yaml_nperiods() -> u32 {
    3
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct DeviceSettingsYaml {
    device_id: String,
    sample_rate: u32,
    buffer_size_frames: u32,
    #[serde(default = "default_yaml_bit_depth")]
    bit_depth: u32,
    // Linux JACK tuning — only emitted on Linux. On macOS/Windows these
    // fields don't exist on DeviceSettings, so serialization skips them
    // and deserialization ignores them if present in a foreign YAML.
    #[cfg(target_os = "linux")]
    #[serde(default = "default_yaml_realtime")]
    realtime: bool,
    #[cfg(target_os = "linux")]
    #[serde(default = "default_yaml_rt_priority")]
    rt_priority: u8,
    #[cfg(target_os = "linux")]
    #[serde(default = "default_yaml_nperiods")]
    nperiods: u32,
}

impl From<DeviceSettingsYaml> for DeviceSettings {
    fn from(value: DeviceSettingsYaml) -> Self {
        Self {
            device_id: DeviceId(value.device_id),
            sample_rate: value.sample_rate,
            buffer_size_frames: value.buffer_size_frames,
            bit_depth: value.bit_depth,
            #[cfg(target_os = "linux")]
            realtime: value.realtime,
            #[cfg(target_os = "linux")]
            rt_priority: value.rt_priority,
            #[cfg(target_os = "linux")]
            nperiods: value.nperiods,
        }
    }
}
