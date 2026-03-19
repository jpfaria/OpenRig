use domain::ids::PresetId;
use serde::{Deserialize, Serialize};

use crate::block::AudioBlock;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupPreset {
    pub id: PresetId,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub blocks: Vec<AudioBlock>,
}
