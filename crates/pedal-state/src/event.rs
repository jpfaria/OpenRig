use pedal_domain::ids::{BlockId, PresetId};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    PresetLoaded { preset_id: PresetId },
    BlockBypassChanged { block_id: BlockId, bypass: bool },
    ParameterValueChanged { parameter_id: String, value: f32 },
    BlockOptionSelected { block_id: BlockId, option_block_id: BlockId },
    TrackGainChanged { track_id: String, value: f32 },
}
