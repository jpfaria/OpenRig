use pedal_domain::ids::{BlockId, PresetId, SetupId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub id: PresetId,
    pub setup_id: SetupId,
    pub name: String,
    pub bypass_by_block: HashMap<BlockId, bool>,
    pub selected_option_by_block: HashMap<BlockId, BlockId>,
    pub parameter_values: HashMap<String, f32>,
}
