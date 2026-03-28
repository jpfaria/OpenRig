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

/// Determines the processing layout from a ChainInput alone.
pub fn processing_layout_for_input(input: &ChainInput) -> ProcessingLayout {
    let ch_count = input.channels.len();
    match input.mode {
        ChainInputMode::DualMono if ch_count >= 2 => ProcessingLayout::DualMono,
        ChainInputMode::Stereo if ch_count >= 2 => ProcessingLayout::Stereo,
        _ => ProcessingLayout::Mono,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainInput {
    #[serde(default = "default_input_name")]
    pub name: String,
    pub device_id: DeviceId,
    #[serde(default)]
    pub mode: ChainInputMode,
    pub channels: Vec<usize>,
}

fn default_input_name() -> String {
    "Input".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainOutput {
    #[serde(default = "default_output_name")]
    pub name: String,
    pub device_id: DeviceId,
    #[serde(default)]
    pub mode: ChainOutputMode,
    pub channels: Vec<usize>,
}

fn default_output_name() -> String {
    "Output".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Chain {
    #[serde(skip)]
    pub id: ChainId,
    #[serde(default)]
    pub description: Option<String>,
    pub instrument: String,
    pub enabled: bool,
    // New multi-input/output fields
    #[serde(default)]
    pub inputs: Vec<ChainInput>,
    #[serde(default)]
    pub outputs: Vec<ChainOutput>,
    #[serde(default)]
    pub blocks: Vec<AudioBlock>,
    // Legacy fields — kept for backward-compatible deserialization, skipped on serialization
    #[serde(default, skip_serializing)]
    pub input_device_id: DeviceId,
    #[serde(default, skip_serializing)]
    pub input_channels: Vec<usize>,
    #[serde(default, skip_serializing)]
    pub output_device_id: DeviceId,
    #[serde(default, skip_serializing)]
    pub output_channels: Vec<usize>,
    #[serde(default, skip_serializing)]
    pub output_mixdown: ChainOutputMixdown,
    #[serde(default, skip_serializing)]
    pub input_mode: ChainInputMode,
}

impl Chain {
    /// Migrate legacy single-input/output to new multi-input/output model.
    /// Called after deserialization.
    pub fn migrate_legacy_io(&mut self) {
        if self.inputs.is_empty() && !self.input_device_id.0.is_empty() {
            self.inputs.push(ChainInput {
                name: "Input 1".to_string(),
                device_id: self.input_device_id.clone(),
                mode: self.input_mode,
                channels: self.input_channels.clone(),
            });
        }
        if self.outputs.is_empty() && !self.output_device_id.0.is_empty() {
            let mode = if self.output_channels.len() >= 2 {
                ChainOutputMode::Stereo
            } else {
                ChainOutputMode::Mono
            };
            self.outputs.push(ChainOutput {
                name: "Output 1".to_string(),
                device_id: self.output_device_id.clone(),
                mode,
                channels: self.output_channels.clone(),
            });
        }
    }

    /// Validate that no two inputs share the same device+channel,
    /// and no two outputs share the same device+channel.
    pub fn validate_channel_conflicts(&self) -> Result<(), String> {
        let mut used: Vec<(String, usize)> = Vec::new();
        for input in &self.inputs {
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
        for output in &self.outputs {
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
