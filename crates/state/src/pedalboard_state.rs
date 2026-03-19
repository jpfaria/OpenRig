use domain::ids::{BlockId, ParameterId, PresetId, SetupId};
use domain::value_objects::ParameterValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PedalboardState {
    pub setup_id: SetupId,
    pub active_preset_id: Option<PresetId>,
    pub bypass_by_block: HashMap<BlockId, bool>,
    pub selected_option_by_block: HashMap<BlockId, BlockId>,
    pub parameter_values: HashMap<ParameterId, ParameterValue>,
}

impl Default for PedalboardState {
    fn default() -> Self {
        Self {
            setup_id: SetupId("default".to_string()),
            active_preset_id: None,
            bypass_by_block: HashMap::new(),
            selected_option_by_block: HashMap::new(),
            parameter_values: HashMap::new(),
        }
    }
}
