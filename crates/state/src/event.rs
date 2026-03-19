use domain::ids::{BlockId, ParameterId, PresetId};
use domain::value_objects::ParameterValue;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    PresetLoaded {
        preset_id: PresetId,
    },
    BlockBypassChanged {
        block_id: BlockId,
        bypass: bool,
    },
    ParameterValueChanged {
        parameter_id: ParameterId,
        value: ParameterValue,
    },
    BlockOptionSelected {
        block_id: BlockId,
        option_block_id: BlockId,
    },
}
