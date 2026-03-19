use domain::ids::{DeviceId, PresetId, TrackId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TrackOutputMixdown {
    Sum,
    #[default]
    Average,
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Track {
    #[serde(skip)]
    pub id: TrackId,
    #[serde(default)]
    pub description: Option<String>,
    pub enabled: bool,
    pub input_device_id: DeviceId,
    pub input_channels: Vec<usize>,
    pub output_device_id: DeviceId,
    pub output_channels: Vec<usize>,
    pub preset_id: PresetId,
    pub output_mixdown: TrackOutputMixdown,
}
