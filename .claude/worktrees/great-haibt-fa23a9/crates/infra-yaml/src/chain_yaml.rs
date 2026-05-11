//! YAML <-> Chain conversion. Mirrors project::chain::Chain to/from the
//! YAML schema. Lifted out of `lib.rs` so the production file stays under
//! the size cap.

use anyhow::Result;
use domain::ids::{BlockId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMixdown, ChainOutputMode};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::block_yaml::{load_audio_block_value, AudioBlockYaml};
use crate::{default_instrument, generated_chain_id};

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
pub(crate) struct ChainInputEntryYaml {
    #[serde(default, skip_serializing)]
    pub(crate) name: String,
    pub(crate) device_id: String,
    #[serde(default)]
    pub(crate) mode: ChainInputMode,
    pub(crate) channels: Vec<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
pub(crate) struct ChainInputYaml {
    #[serde(default = "default_io_yaml_model")]
    model: String,
    // Legacy: name field migrated to entry-level
    #[serde(default, skip_serializing)]
    name: String,
    // New format: entries list
    #[serde(default)]
    entries: Vec<ChainInputEntryYaml>,
    // Legacy format: single device_id/mode/channels (for backward compat)
    #[serde(default, skip_serializing)]
    device_id: Option<String>,
    #[serde(default, skip_serializing)]
    mode: Option<ChainInputMode>,
    #[serde(default, skip_serializing)]
    channels: Option<Vec<usize>>,
}

pub(crate) fn default_io_yaml_model() -> String {
    "standard".to_string()
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
pub(crate) struct ChainOutputEntryYaml {
    #[serde(default, skip_serializing)]
    pub(crate) name: String,
    pub(crate) device_id: String,
    #[serde(default)]
    pub(crate) mode: ChainOutputMode,
    pub(crate) channels: Vec<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
pub(crate) struct ChainOutputYaml {
    #[serde(default = "default_io_yaml_model")]
    model: String,
    // Legacy: name field migrated to entry-level
    #[serde(default, skip_serializing)]
    name: String,
    // New format: entries list
    #[serde(default)]
    entries: Vec<ChainOutputEntryYaml>,
    // Legacy format: single device_id/mode/channels (for backward compat)
    #[serde(default, skip_serializing)]
    device_id: Option<String>,
    #[serde(default, skip_serializing)]
    mode: Option<ChainOutputMode>,
    #[serde(default, skip_serializing)]
    channels: Option<Vec<usize>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
pub(crate) struct ChainYaml {
    #[serde(default)]
    description: Option<String>,
    #[serde(default = "default_instrument")]
    instrument: String,
    #[serde(default, skip_serializing)]
    enabled: bool,
    // Legacy multi-input/output fields — kept for backward-compatible deserialization, skipped on serialization
    #[serde(default, skip_serializing)]
    inputs: Vec<ChainInputYaml>,
    #[serde(default, skip_serializing)]
    outputs: Vec<ChainOutputYaml>,
    // Legacy fields — kept for backward-compatible deserialization, skipped on serialization
    #[serde(default, skip_serializing)]
    input_device_id: Option<String>,
    #[serde(default, skip_serializing)]
    input_channels: Option<Vec<usize>>,
    #[serde(default, skip_serializing)]
    output_device_id: Option<String>,
    #[serde(default, skip_serializing)]
    output_channels: Option<Vec<usize>>,
    #[serde(default)]
    blocks: Vec<Value>,
    #[serde(default, skip_serializing)]
    output_mixdown: ChainOutputMixdown,
    #[serde(default, skip_serializing)]
    input_mode: ChainInputMode,
}

impl ChainYaml {
    pub(crate) fn into_chain(self, index: usize) -> Result<Chain> {
        let chain_id = generated_chain_id(index);
        log::debug!(
            "deserializing chain index={}, description={:?}, instrument='{}', enabled={}",
            index,
            self.description,
            self.instrument,
            self.enabled
        );

        // Parse all blocks from the blocks array (new format may include input/output blocks inline)
        let parsed_blocks: Vec<AudioBlock> = self
            .blocks
            .into_iter()
            .enumerate()
            .filter_map(|(block_index, block)| {
                load_audio_block_value(block, &chain_id, block_index)
            })
            .collect();

        // Check if blocks already contain Input/Output (new inline format)
        let has_inline_inputs = parsed_blocks
            .iter()
            .any(|b| matches!(&b.kind, AudioBlockKind::Input(_)));
        let has_inline_outputs = parsed_blocks
            .iter()
            .any(|b| matches!(&b.kind, AudioBlockKind::Output(_)));

        if has_inline_inputs || has_inline_outputs {
            // New format: blocks already contain I/O inline, use as-is
            let mut chain = Chain {
                id: chain_id.clone(),
                description: self.description,
                instrument: self.instrument,
                enabled: self.enabled,
                blocks: parsed_blocks,
            };
            // Migrate projects saved while issue #377 was open: split-per-device
            // I/O runs at chain head/tail collapse back into a single block.
            chain.coalesce_endpoint_blocks();
            return Ok(chain);
        }

        // Old format: convert separate inputs/outputs sections to blocks
        let mut input_blocks: Vec<AudioBlock> = self
            .inputs
            .into_iter()
            .enumerate()
            .map(|(i, inp)| {
                let entries = if !inp.entries.is_empty() {
                    inp.entries
                        .into_iter()
                        .map(|e| InputEntry {
                            device_id: DeviceId(e.device_id),
                            mode: e.mode,
                            channels: e.channels,
                        })
                        .collect()
                } else if let Some(device_id) = inp.device_id {
                    vec![InputEntry {
                        device_id: DeviceId(device_id),
                        mode: inp.mode.unwrap_or_default(),
                        channels: inp.channels.unwrap_or_default(),
                    }]
                } else {
                    Vec::new()
                };
                AudioBlock {
                    id: BlockId(format!("{}:input:{}", chain_id.0, i)),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: inp.model,
                        entries,
                    }),
                }
            })
            .collect();

        let mut output_blocks: Vec<AudioBlock> = self
            .outputs
            .into_iter()
            .enumerate()
            .map(|(i, out)| {
                let entries = if !out.entries.is_empty() {
                    out.entries
                        .into_iter()
                        .map(|e| OutputEntry {
                            device_id: DeviceId(e.device_id),
                            mode: e.mode,
                            channels: e.channels,
                        })
                        .collect()
                } else if let Some(device_id) = out.device_id {
                    vec![OutputEntry {
                        device_id: DeviceId(device_id),
                        mode: out.mode.unwrap_or_default(),
                        channels: out.channels.unwrap_or_default(),
                    }]
                } else {
                    Vec::new()
                };
                AudioBlock {
                    id: BlockId(format!("{}:output:{}", chain_id.0, i)),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: out.model,
                        entries,
                    }),
                }
            })
            .collect();

        // Oldest legacy format: single input_device_id/output_device_id fields
        if input_blocks.is_empty() {
            let legacy_device = self.input_device_id.unwrap_or_default();
            if !legacy_device.is_empty() {
                input_blocks.push(AudioBlock {
                    id: BlockId(format!("{}:input:0", chain_id.0)),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".to_string(),
                        entries: vec![InputEntry {
                            device_id: DeviceId(legacy_device),
                            mode: self.input_mode,
                            channels: self.input_channels.unwrap_or_default(),
                        }],
                    }),
                });
            }
        }
        if output_blocks.is_empty() {
            let legacy_device = self.output_device_id.unwrap_or_default();
            if !legacy_device.is_empty() {
                let legacy_channels = self.output_channels.unwrap_or_default();
                let mode = if legacy_channels.len() >= 2 {
                    ChainOutputMode::Stereo
                } else {
                    ChainOutputMode::Mono
                };
                output_blocks.push(AudioBlock {
                    id: BlockId(format!("{}:output:0", chain_id.0)),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".to_string(),
                        entries: vec![OutputEntry {
                            device_id: DeviceId(legacy_device),
                            mode,
                            channels: legacy_channels,
                        }],
                    }),
                });
            }
        }

        // Build blocks: inputs first, then audio blocks, then outputs
        let mut all_blocks =
            Vec::with_capacity(input_blocks.len() + parsed_blocks.len() + output_blocks.len());
        all_blocks.extend(input_blocks);
        all_blocks.extend(parsed_blocks);
        all_blocks.extend(output_blocks);

        let mut chain = Chain {
            id: chain_id.clone(),
            description: self.description,
            instrument: self.instrument,
            enabled: self.enabled,
            blocks: all_blocks,
        };
        chain.coalesce_endpoint_blocks();
        Ok(chain)
    }

    pub(crate) fn from_chain(chain: &Chain) -> Result<Self> {
        // All blocks (including I/O) go into the blocks array
        let audio_blocks: Vec<Value> = chain
            .blocks
            .iter()
            .map(|block| {
                Ok(serde_yaml::to_value(AudioBlockYaml::from_audio_block(
                    block,
                )?)?)
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            description: chain.description.clone(),
            instrument: chain.instrument.clone(),
            enabled: false, // chains always start disabled on project load, regardless of saved state
            inputs: Vec::new(),
            outputs: Vec::new(),
            input_device_id: None,
            input_channels: None,
            output_device_id: None,
            output_channels: None,
            blocks: audio_blocks,
            output_mixdown: ChainOutputMixdown::Average,
            input_mode: ChainInputMode::default(),
        })
    }
}
