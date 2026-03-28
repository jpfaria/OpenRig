use domain::ids::ChainId;
use serde::{Deserialize, Serialize};

use crate::block::{AudioBlock, AudioBlockKind, InputBlock, OutputBlock};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChainOutputMode {
    Mono,
    #[default]
    Stereo,
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

/// Determines the processing layout from an InputBlock alone.
pub fn processing_layout_for_input(input: &InputBlock) -> ProcessingLayout {
    let ch_count = input.channels.len();
    match input.mode {
        ChainInputMode::DualMono if ch_count >= 2 => ProcessingLayout::DualMono,
        ChainInputMode::Stereo if ch_count >= 2 => ProcessingLayout::Stereo,
        _ => ProcessingLayout::Mono,
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
    #[serde(default)]
    pub blocks: Vec<AudioBlock>,
}

impl Chain {
    /// Returns all Input blocks with their indices in the blocks vec.
    pub fn input_blocks(&self) -> Vec<(usize, &InputBlock)> {
        self.blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| match &b.kind {
                AudioBlockKind::Input(input) => Some((i, input)),
                _ => None,
            })
            .collect()
    }

    /// Returns all Output blocks with their indices in the blocks vec.
    pub fn output_blocks(&self) -> Vec<(usize, &OutputBlock)> {
        self.blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| match &b.kind {
                AudioBlockKind::Output(output) => Some((i, output)),
                _ => None,
            })
            .collect()
    }

    /// Returns the first Input block, if any.
    pub fn first_input(&self) -> Option<&InputBlock> {
        self.blocks.iter().find_map(|b| match &b.kind {
            AudioBlockKind::Input(input) => Some(input),
            _ => None,
        })
    }

    /// Returns the last Output block, if any.
    pub fn last_output(&self) -> Option<&OutputBlock> {
        self.blocks.iter().rev().find_map(|b| match &b.kind {
            AudioBlockKind::Output(output) => Some(output),
            _ => None,
        })
    }

    /// Validate that no two inputs share the same device+channel,
    /// and no two outputs share the same device+channel.
    pub fn validate_channel_conflicts(&self) -> Result<(), String> {
        let mut used: Vec<(String, usize)> = Vec::new();
        for (_, input) in self.input_blocks() {
            for &ch in &input.channels {
                let key = (input.device_id.0.clone(), ch);
                if used.contains(&key) {
                    return Err(format!(
                        "Channel {} on device '{}' is used by multiple inputs",
                        ch, input.device_id.0
                    ));
                }
                used.push(key);
            }
        }
        let mut used: Vec<(String, usize)> = Vec::new();
        for (_, output) in self.output_blocks() {
            for &ch in &output.channels {
                let key = (output.device_id.0.clone(), ch);
                if used.contains(&key) {
                    return Err(format!(
                        "Channel {} on device '{}' is used by multiple outputs",
                        ch, output.device_id.0
                    ));
                }
                used.push(key);
            }
        }
        Ok(())
    }
}
