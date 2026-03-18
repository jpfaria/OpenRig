use pedal_domain::ids::{BlockId, PresetId, SetupId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PedalboardState {
    pub setup_id: SetupId,
    pub active_preset_id: Option<PresetId>,
    pub bypass_by_block: HashMap<BlockId, bool>,
    pub selected_option_by_block: HashMap<BlockId, BlockId>,
    pub parameter_values: HashMap<String, f32>,
}
