use domain::ids::{ChainId, DeviceId};
use serde::{Deserialize, Serialize};

use crate::block::AudioBlock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChainOutputMixdown {
    Sum,
    #[default]
    Average,
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Chain {
    #[serde(skip)]
    pub id: ChainId,
    #[serde(default)]
    pub description: Option<String>,
    pub instrument: String,
    pub enabled: bool,
    pub input_device_id: DeviceId,
    pub input_channels: Vec<usize>,
    pub output_device_id: DeviceId,
    pub output_channels: Vec<usize>,
    #[serde(default)]
    pub blocks: Vec<AudioBlock>,
    pub output_mixdown: ChainOutputMixdown,
}
