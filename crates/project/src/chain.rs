use domain::ids::ChainId;
use serde::{Deserialize, Serialize};

use crate::block::{AudioBlock, AudioBlockKind, InputBlock, InputEntry, InsertBlock, OutputBlock};

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

/// Determines the processing layout from an InputEntry.
pub fn processing_layout_for_input_entry(entry: &InputEntry) -> ProcessingLayout {
    let ch_count = entry.channels.len();
    match entry.mode {
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

    /// Returns all Insert blocks with their indices in the blocks vec.
    pub fn insert_blocks(&self) -> Vec<(usize, &InsertBlock)> {
        self.blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| match &b.kind {
                AudioBlockKind::Insert(insert) => Some((i, insert)),
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

    /// Validate that no two input entries share the same device+channel,
    /// and no two output entries share the same device+channel.
    pub fn validate_channel_conflicts(&self) -> Result<(), String> {
        let mut used: Vec<(String, usize)> = Vec::new();
        for (_, input) in self.input_blocks() {
            for entry in &input.entries {
                for &ch in &entry.channels {
                    let key = (entry.device_id.0.clone(), ch);
                    if used.contains(&key) {
                        return Err(format!(
                            "Channel {} on device '{}' is used by multiple inputs",
                            ch, entry.device_id.0
                        ));
                    }
                    used.push(key);
                }
            }
        }
        let mut used: Vec<(String, usize)> = Vec::new();
        for (_, output) in self.output_blocks() {
            for entry in &output.entries {
                for &ch in &entry.channels {
                    let key = (entry.device_id.0.clone(), ch);
                    if used.contains(&key) {
                        return Err(format!(
                            "Channel {} on device '{}' is used by multiple outputs",
                            ch, entry.device_id.0
                        ));
                    }
                    used.push(key);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{
        AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, InsertBlock,
        InsertEndpoint, OutputBlock, OutputEntry, schema_for_block_model,
    };
    use crate::param::ParameterSet;
    use domain::ids::{BlockId, ChainId, DeviceId};

    fn make_input_block(id: &str, device: &str, channels: Vec<usize>, mode: ChainInputMode) -> AudioBlock {
        AudioBlock {
            id: BlockId(id.into()),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".to_string(),
                entries: vec![InputEntry {
                    name: "Input".to_string(),
                    device_id: DeviceId(device.into()),
                    mode,
                    channels,
                }],
            }),
        }
    }

    fn make_output_block(id: &str, device: &str, channels: Vec<usize>, mode: ChainOutputMode) -> AudioBlock {
        AudioBlock {
            id: BlockId(id.into()),
            enabled: true,
            kind: AudioBlockKind::Output(OutputBlock {
                model: "standard".to_string(),
                entries: vec![OutputEntry {
                    name: "Output".to_string(),
                    device_id: DeviceId(device.into()),
                    mode,
                    channels,
                }],
            }),
        }
    }

    fn make_insert_block(id: &str) -> AudioBlock {
        AudioBlock {
            id: BlockId(id.into()),
            enabled: true,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".to_string(),
                send: InsertEndpoint {
                    device_id: DeviceId("send-dev".into()),
                    mode: ChainInputMode::Stereo,
                    channels: vec![0, 1],
                },
                return_: InsertEndpoint {
                    device_id: DeviceId("return-dev".into()),
                    mode: ChainInputMode::Stereo,
                    channels: vec![0, 1],
                },
            }),
        }
    }

    fn make_delay_block(id: &str) -> AudioBlock {
        let model = block_delay::supported_models().first().unwrap();
        let schema = schema_for_block_model("delay", model).unwrap();
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .unwrap();
        AudioBlock {
            id: BlockId(id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "delay".to_string(),
                model: model.to_string(),
                params,
            }),
        }
    }

    fn make_chain(blocks: Vec<AudioBlock>) -> Chain {
        Chain {
            id: ChainId("chain:0".to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks,
        }
    }

    // --- processing_layout tests ---

    #[test]
    fn processing_layout_mono_input_mono_output() {
        let layout = processing_layout(&[0], &[0], ChainInputMode::Mono);
        assert_eq!(layout, ProcessingLayout::Mono);
    }

    #[test]
    fn processing_layout_mono_input_stereo_output() {
        let layout = processing_layout(&[0], &[0, 1], ChainInputMode::Mono);
        assert_eq!(layout, ProcessingLayout::Stereo);
    }

    #[test]
    fn processing_layout_stereo_input_mono_output() {
        let layout = processing_layout(&[0, 1], &[0], ChainInputMode::Stereo);
        assert_eq!(layout, ProcessingLayout::Stereo);
    }

    #[test]
    fn processing_layout_stereo_input_stereo_output() {
        let layout = processing_layout(&[0, 1], &[0, 1], ChainInputMode::Stereo);
        assert_eq!(layout, ProcessingLayout::Stereo);
    }

    #[test]
    fn processing_layout_dual_mono_two_inputs() {
        let layout = processing_layout(&[0, 1], &[0, 1], ChainInputMode::DualMono);
        assert_eq!(layout, ProcessingLayout::DualMono);
    }

    #[test]
    fn processing_layout_dual_mono_single_input_falls_to_mono() {
        // DualMono with only 1 input channel => not enough for DualMono, falls through
        let layout = processing_layout(&[0], &[0], ChainInputMode::DualMono);
        // With 1 input and DualMono mode, in_count < 2, so it skips DualMono check
        // Not stereo mode, so it goes to out_count match: 1 => Mono
        assert_eq!(layout, ProcessingLayout::Mono);
    }

    #[test]
    fn processing_layout_mono_input_empty_output() {
        let layout = processing_layout(&[0], &[], ChainInputMode::Mono);
        assert_eq!(layout, ProcessingLayout::Mono);
    }

    #[test]
    fn processing_layout_stereo_mode_single_channel_still_stereo() {
        // Stereo mode overrides channel count
        let layout = processing_layout(&[0], &[0], ChainInputMode::Stereo);
        assert_eq!(layout, ProcessingLayout::Stereo);
    }

    // --- processing_layout_for_input_entry tests ---

    #[test]
    fn processing_layout_for_input_entry_mono() {
        let entry = InputEntry {
            name: "In".to_string(),
            device_id: DeviceId("dev".into()),
            mode: ChainInputMode::Mono,
            channels: vec![0],
        };
        assert_eq!(processing_layout_for_input_entry(&entry), ProcessingLayout::Mono);
    }

    #[test]
    fn processing_layout_for_input_entry_stereo() {
        let entry = InputEntry {
            name: "In".to_string(),
            device_id: DeviceId("dev".into()),
            mode: ChainInputMode::Stereo,
            channels: vec![0, 1],
        };
        assert_eq!(processing_layout_for_input_entry(&entry), ProcessingLayout::Stereo);
    }

    #[test]
    fn processing_layout_for_input_entry_dual_mono() {
        let entry = InputEntry {
            name: "In".to_string(),
            device_id: DeviceId("dev".into()),
            mode: ChainInputMode::DualMono,
            channels: vec![0, 1],
        };
        assert_eq!(processing_layout_for_input_entry(&entry), ProcessingLayout::DualMono);
    }

    #[test]
    fn processing_layout_for_input_entry_stereo_single_channel_falls_to_mono() {
        let entry = InputEntry {
            name: "In".to_string(),
            device_id: DeviceId("dev".into()),
            mode: ChainInputMode::Stereo,
            channels: vec![0], // only 1 channel despite Stereo mode
        };
        assert_eq!(processing_layout_for_input_entry(&entry), ProcessingLayout::Mono);
    }

    #[test]
    fn processing_layout_for_input_entry_dual_mono_single_channel_falls_to_mono() {
        let entry = InputEntry {
            name: "In".to_string(),
            device_id: DeviceId("dev".into()),
            mode: ChainInputMode::DualMono,
            channels: vec![0], // only 1 channel despite DualMono mode
        };
        assert_eq!(processing_layout_for_input_entry(&entry), ProcessingLayout::Mono);
    }

    // --- Chain::input_blocks / output_blocks / insert_blocks ---

    #[test]
    fn input_blocks_returns_all_inputs_with_indices() {
        let chain = make_chain(vec![
            make_input_block("in:0", "dev", vec![0], ChainInputMode::Mono),
            make_delay_block("fx:0"),
            make_input_block("in:1", "dev2", vec![0], ChainInputMode::Mono),
            make_output_block("out:0", "dev", vec![0, 1], ChainOutputMode::Stereo),
        ]);
        let inputs = chain.input_blocks();
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0].0, 0);
        assert_eq!(inputs[1].0, 2);
    }

    #[test]
    fn output_blocks_returns_all_outputs_with_indices() {
        let chain = make_chain(vec![
            make_input_block("in:0", "dev", vec![0], ChainInputMode::Mono),
            make_output_block("out:0", "dev", vec![0, 1], ChainOutputMode::Stereo),
            make_delay_block("fx:0"),
            make_output_block("out:1", "dev2", vec![0, 1], ChainOutputMode::Stereo),
        ]);
        let outputs = chain.output_blocks();
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].0, 1);
        assert_eq!(outputs[1].0, 3);
    }

    #[test]
    fn insert_blocks_returns_all_inserts_with_indices() {
        let chain = make_chain(vec![
            make_input_block("in:0", "dev", vec![0], ChainInputMode::Mono),
            make_insert_block("ins:0"),
            make_delay_block("fx:0"),
            make_insert_block("ins:1"),
            make_output_block("out:0", "dev", vec![0, 1], ChainOutputMode::Stereo),
        ]);
        let inserts = chain.insert_blocks();
        assert_eq!(inserts.len(), 2);
        assert_eq!(inserts[0].0, 1);
        assert_eq!(inserts[1].0, 3);
    }

    #[test]
    fn input_blocks_empty_chain_returns_empty() {
        let chain = make_chain(vec![]);
        assert!(chain.input_blocks().is_empty());
    }

    #[test]
    fn output_blocks_empty_chain_returns_empty() {
        let chain = make_chain(vec![]);
        assert!(chain.output_blocks().is_empty());
    }

    #[test]
    fn insert_blocks_no_inserts_returns_empty() {
        let chain = make_chain(vec![
            make_input_block("in:0", "dev", vec![0], ChainInputMode::Mono),
            make_output_block("out:0", "dev", vec![0, 1], ChainOutputMode::Stereo),
        ]);
        assert!(chain.insert_blocks().is_empty());
    }

    // --- Chain::first_input / last_output ---

    #[test]
    fn first_input_returns_first_input_block() {
        let chain = make_chain(vec![
            make_input_block("in:0", "dev-a", vec![0], ChainInputMode::Mono),
            make_delay_block("fx:0"),
            make_input_block("in:1", "dev-b", vec![1], ChainInputMode::Stereo),
            make_output_block("out:0", "dev", vec![0, 1], ChainOutputMode::Stereo),
        ]);
        let first = chain.first_input().expect("should have first input");
        assert_eq!(first.entries[0].device_id.0, "dev-a");
    }

    #[test]
    fn first_input_empty_chain_returns_none() {
        let chain = make_chain(vec![]);
        assert!(chain.first_input().is_none());
    }

    #[test]
    fn first_input_no_input_blocks_returns_none() {
        let chain = make_chain(vec![
            make_delay_block("fx:0"),
            make_output_block("out:0", "dev", vec![0, 1], ChainOutputMode::Stereo),
        ]);
        assert!(chain.first_input().is_none());
    }

    #[test]
    fn last_output_returns_last_output_block() {
        let chain = make_chain(vec![
            make_input_block("in:0", "dev", vec![0], ChainInputMode::Mono),
            make_output_block("out:0", "dev-a", vec![0, 1], ChainOutputMode::Stereo),
            make_delay_block("fx:0"),
            make_output_block("out:1", "dev-b", vec![0, 1], ChainOutputMode::Stereo),
        ]);
        let last = chain.last_output().expect("should have last output");
        assert_eq!(last.entries[0].device_id.0, "dev-b");
    }

    #[test]
    fn last_output_empty_chain_returns_none() {
        let chain = make_chain(vec![]);
        assert!(chain.last_output().is_none());
    }

    #[test]
    fn last_output_no_output_blocks_returns_none() {
        let chain = make_chain(vec![
            make_input_block("in:0", "dev", vec![0], ChainInputMode::Mono),
            make_delay_block("fx:0"),
        ]);
        assert!(chain.last_output().is_none());
    }

    // --- Chain::validate_channel_conflicts ---

    #[test]
    fn validate_channel_conflicts_no_conflict_ok() {
        let chain = make_chain(vec![
            make_input_block("in:0", "dev", vec![0], ChainInputMode::Mono),
            make_output_block("out:0", "dev", vec![0, 1], ChainOutputMode::Stereo),
        ]);
        assert!(chain.validate_channel_conflicts().is_ok());
    }

    #[test]
    fn validate_channel_conflicts_input_conflict_detected() {
        let chain = Chain {
            id: ChainId("chain:0".to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![
                AudioBlock {
                    id: BlockId("in:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".to_string(),
                        entries: vec![InputEntry {
                            name: "A".to_string(),
                            device_id: DeviceId("scarlett".into()),
                            mode: ChainInputMode::Mono,
                            channels: vec![0],
                        }],
                    }),
                },
                AudioBlock {
                    id: BlockId("in:1".into()),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".to_string(),
                        entries: vec![InputEntry {
                            name: "B".to_string(),
                            device_id: DeviceId("scarlett".into()),
                            mode: ChainInputMode::Mono,
                            channels: vec![0], // same device+channel as in:0
                        }],
                    }),
                },
                make_output_block("out:0", "dev", vec![0, 1], ChainOutputMode::Stereo),
            ],
        };
        let err = chain.validate_channel_conflicts().unwrap_err();
        assert!(err.contains("Channel 0"));
        assert!(err.contains("multiple inputs"));
    }

    #[test]
    fn validate_channel_conflicts_output_conflict_detected() {
        let chain = Chain {
            id: ChainId("chain:0".to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![
                make_input_block("in:0", "dev", vec![0], ChainInputMode::Mono),
                AudioBlock {
                    id: BlockId("out:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".to_string(),
                        entries: vec![OutputEntry {
                            name: "A".to_string(),
                            device_id: DeviceId("speakers".into()),
                            mode: ChainOutputMode::Stereo,
                            channels: vec![0, 1],
                        }],
                    }),
                },
                AudioBlock {
                    id: BlockId("out:1".into()),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".to_string(),
                        entries: vec![OutputEntry {
                            name: "B".to_string(),
                            device_id: DeviceId("speakers".into()),
                            mode: ChainOutputMode::Mono,
                            channels: vec![1], // conflicts with out:0 channel 1
                        }],
                    }),
                },
            ],
        };
        let err = chain.validate_channel_conflicts().unwrap_err();
        assert!(err.contains("Channel 1"));
        assert!(err.contains("multiple outputs"));
    }

    #[test]
    fn validate_channel_conflicts_input_and_output_same_channel_ok() {
        // Input and output can use the same device+channel (different directions)
        let chain = make_chain(vec![
            make_input_block("in:0", "scarlett", vec![0], ChainInputMode::Mono),
            make_output_block("out:0", "scarlett", vec![0], ChainOutputMode::Mono),
        ]);
        assert!(chain.validate_channel_conflicts().is_ok());
    }

    #[test]
    fn validate_channel_conflicts_different_devices_ok() {
        let chain = Chain {
            id: ChainId("chain:0".to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![
                AudioBlock {
                    id: BlockId("in:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".to_string(),
                        entries: vec![InputEntry {
                            name: "A".to_string(),
                            device_id: DeviceId("scarlett".into()),
                            mode: ChainInputMode::Mono,
                            channels: vec![0],
                        }],
                    }),
                },
                AudioBlock {
                    id: BlockId("in:1".into()),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".to_string(),
                        entries: vec![InputEntry {
                            name: "B".to_string(),
                            device_id: DeviceId("macbook".into()),
                            mode: ChainInputMode::Mono,
                            channels: vec![0], // same channel but different device
                        }],
                    }),
                },
                make_output_block("out:0", "dev", vec![0, 1], ChainOutputMode::Stereo),
            ],
        };
        assert!(chain.validate_channel_conflicts().is_ok());
    }

    #[test]
    fn validate_channel_conflicts_empty_chain_ok() {
        let chain = make_chain(vec![]);
        assert!(chain.validate_channel_conflicts().is_ok());
    }

    #[test]
    fn validate_channel_conflicts_within_single_input_multi_entry() {
        // Two entries in the same InputBlock sharing a channel
        let chain = Chain {
            id: ChainId("chain:0".to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![
                AudioBlock {
                    id: BlockId("in:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".to_string(),
                        entries: vec![
                            InputEntry {
                                name: "A".to_string(),
                                device_id: DeviceId("dev".into()),
                                mode: ChainInputMode::Mono,
                                channels: vec![0],
                            },
                            InputEntry {
                                name: "B".to_string(),
                                device_id: DeviceId("dev".into()),
                                mode: ChainInputMode::Mono,
                                channels: vec![0], // duplicate within same InputBlock
                            },
                        ],
                    }),
                },
                make_output_block("out:0", "dev", vec![0, 1], ChainOutputMode::Stereo),
            ],
        };
        let err = chain.validate_channel_conflicts().unwrap_err();
        assert!(err.contains("Channel 0"));
    }

    // --- ChainInputMode / ChainOutputMode defaults ---

    #[test]
    fn chain_input_mode_default_is_mono() {
        assert_eq!(ChainInputMode::default(), ChainInputMode::Mono);
    }

    #[test]
    fn chain_output_mode_default_is_stereo() {
        assert_eq!(ChainOutputMode::default(), ChainOutputMode::Stereo);
    }

    #[test]
    fn chain_output_mixdown_default_is_average() {
        assert_eq!(ChainOutputMixdown::default(), ChainOutputMixdown::Average);
    }
}
