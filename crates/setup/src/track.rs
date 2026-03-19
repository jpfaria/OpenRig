use domain::ids::{InputId, OutputId, TrackId};
use serde::{Deserialize, Serialize};

use crate::block::AudioBlock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TrackOutputMixdown {
    Sum,
    #[default]
    Average,
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub input_id: InputId,
    pub output_ids: Vec<OutputId>,
    pub output_mixdown: TrackOutputMixdown,
    pub gain: f32,
    pub blocks: Vec<AudioBlock>,
}

const fn default_enabled() -> bool {
    true
}
