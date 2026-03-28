use domain::ids::{ChainId, DeviceId};
use serde::{Deserialize, Serialize};

use crate::block::AudioBlock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChainOutputMixdown {
    Sum,
    #[default]
    Average,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChainInputMode {
    /// Single-channel input; upmixed to stereo for stereo outputs.
    #[default]
    #[serde(alias = "auto")]
    Mono,
    /// Two-channel input treated as a true stereo L/R pair.
    Stereo,
    /// Two independent mono pipelines (e.g. two guitars on separate inputs).
    DualMono,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingLayout {
    Mono,
    Stereo,
    DualMono,
}

/// Determines the audio processing layout from the combination
/// of input channels, output channels, and input mode.
pub fn processing_layout(
    input_channels: &[usize],
    output_channels: &[usize],
    input_mode: ChainInputMode,
) -> ProcessingLayout {
    let in_count = input_channels.len();
    let out_count = output_channels.len();

    // Dual mono: 2 independent mono pipelines
    if in_count >= 2 && matches!(input_mode, ChainInputMode::DualMono) {
        return ProcessingLayout::DualMono;
    }

    // Stereo input: always process as stereo
    if matches!(input_mode, ChainInputMode::Stereo) {
        return ProcessingLayout::Stereo;
    }

    // Mono input: output channel count determines final layout (upmix if needed)
    match out_count {
        0 | 1 => ProcessingLayout::Mono,
        _ => ProcessingLayout::Stereo,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Chain {
    #[serde(skip)]
    pub id: ChainId,
    #[serde(default)]
    pub description: Option<String>,
    pub instrument: String,
    pub enabled: bool,
    pub input_device_id: DeviceId,
    pub input_channels: Vec<usize>,
    pub output_device_id: DeviceId,
    pub output_channels: Vec<usize>,
    #[serde(default)]
    pub blocks: Vec<AudioBlock>,
    pub output_mixdown: ChainOutputMixdown,
    #[serde(default)]
    pub input_mode: ChainInputMode,
}
