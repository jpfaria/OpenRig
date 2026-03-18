use domain::ids::{BlockId, ParameterId, PresetId};
use domain::value_objects::ParameterValue;

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    LoadPreset {
        preset_id: PresetId,
    },
    SetBlockBypass {
        block_id: BlockId,
        bypass: bool,
    },
    SetParameterValue {
        parameter_id: ParameterId,
        value: ParameterValue,
    },
    SelectBlockOption {
        block_id: BlockId,
        option_block_id: BlockId,
    },
    SetTrackGain {
        track_id: String,
        value: f32,
    },
}
