use domain::ids::{InputId, OutputId, TrackId};
use serde::{Deserialize, Serialize};

use crate::block::AudioBlock;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub input_id: InputId,
    pub output_ids: Vec<OutputId>,
    pub gain: f32,
    pub blocks: Vec<AudioBlock>,
}
