use pedal_domain::ids::DeviceId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputDevice {
    pub id: DeviceId,
    pub match_name: String,
    pub sample_rate: u32,
    pub buffer_size_frames: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputDevice {
    pub id: DeviceId,
    pub match_name: String,
    pub sample_rate: u32,
    pub buffer_size_frames: u32,
}
