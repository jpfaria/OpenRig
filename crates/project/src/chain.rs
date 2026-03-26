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
    #[default]
    Auto,
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

    // Dual mono is explicit — 2 independent mono pipelines
    if in_count >= 2 && matches!(input_mode, ChainInputMode::DualMono) {
        return ProcessingLayout::DualMono;
    }

    // Otherwise, output determines processing mode
    match (in_count, out_count) {
        (_, 0) => ProcessingLayout::Mono, // no output, default mono
        (_, 1) => {
            if in_count >= 2 {
                ProcessingLayout::Stereo // stereo in → stereo processing → mixdown at output
            } else {
                ProcessingLayout::Mono
            }
        }
        (_, _) => ProcessingLayout::Stereo, // 2+ outputs → stereo processing
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
