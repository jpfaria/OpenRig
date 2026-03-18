use pedal_domain::ids::{DeviceId, InputId, OutputId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Input {
    pub id: InputId,
    pub device_id: DeviceId,
    pub channels: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Output {
    pub id: OutputId,
    pub device_id: DeviceId,
    pub channels: Vec<usize>,
}
