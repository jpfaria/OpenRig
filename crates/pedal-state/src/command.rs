use pedal_domain::ids::{BlockId, PresetId};

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    LoadPreset { preset_id: PresetId },
    SetBlockBypass { block_id: BlockId, bypass: bool },
    SetParameterValue { parameter_id: String, value: f32 },
    SelectBlockOption { block_id: BlockId, option_block_id: BlockId },
    SetTrackGain { track_id: String, value: f32 },
}
