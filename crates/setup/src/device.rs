use domain::ids::DeviceId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputDevice {
    pub device_id: DeviceId,
    pub sample_rate: u32,
    pub buffer_size_frames: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputDevice {
    pub device_id: DeviceId,
    pub sample_rate: u32,
    pub buffer_size_frames: u32,
}
