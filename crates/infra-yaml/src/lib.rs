use anyhow::{anyhow, Context, Result};
use domain::ids::{BlockId, DeviceId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{
    normalize_block_params, AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, InsertBlock, InsertEndpoint, NamBlock, OutputBlock, OutputEntry, SelectBlock,
};
use project::device::DeviceSettings;
use project::param::ParameterSet;
use project::project::Project;
use project::chain::{Chain, ChainInputMode, ChainOutputMixdown, ChainOutputMode};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub struct YamlProjectRepository {
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ChainBlocksPreset {
    pub id: String,
    pub name: Option<String>,
    pub blocks: Vec<project::block::AudioBlock>,
}

pub fn load_chain_preset_file(path: &Path) -> Result<ChainBlocksPreset> {
    log::info!("loading chain preset from {:?}", path);
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read preset yaml {:?}", path))?;
    let dto: PresetYaml = serde_yaml::from_str(&raw)
        .with_context(|| format!("failed to parse preset yaml {:?}", path))?;
    dto.into_preset()
}

pub fn save_chain_preset_file(path: &Path, preset: &ChainBlocksPreset) -> Result<()> {
    log::info!("saving chain preset to {:?}", path);
    let dto = PresetYaml::from_chain_preset(preset)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_yaml::to_string(&dto)?)?;
    Ok(())
}

pub fn serialize_project(project: &Project) -> Result<String> {
    let dto = ProjectYaml::from_project(project)?;
    Ok(serde_yaml::to_string(&dto)?)
}

pub fn serialize_audio_blocks(blocks: &[project::block::AudioBlock]) -> Result<Vec<Value>> {
    blocks
        .iter()
        .map(|block| {
            Ok(serde_yaml::to_value(AudioBlockYaml::from_audio_block(
                block,
            )?)?)
        })
        .collect()
}

impl YamlProjectRepository {
    pub fn load_current_project(&self) -> Result<Project> {
        log::info!("loading project from {:?}", self.path);
        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read yaml {:?}", self.path))?;
        let dto: ProjectYaml = serde_yaml::from_str(&raw)?;
        let project = dto.into_project()?;
        log::debug!("project loaded: {} chains", project.chains.len());
        Ok(project)
    }

    pub fn save_project(&self, project: &Project) -> Result<()> {
        log::info!("saving project to {:?}", self.path);
        let dto = ProjectYaml::from_project(project)?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, serde_yaml::to_string(&dto)?)?;
        log::debug!("project saved: {} chains", project.chains.len());
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct ProjectYaml {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, skip_serializing)]
    device_settings: Vec<DeviceSettingsYaml>,
    chains: Vec<ChainYaml>,
}

impl ProjectYaml {
    fn into_project(self) -> Result<Project> {
        Ok(Project {
            name: self.name,
            device_settings: self.device_settings.into_iter().map(Into::into).collect(),
            chains: self
                .chains
                .into_iter()
                .enumerate()
                .map(|(index, chain)| chain.into_chain(index))
                .collect::<Result<Vec<_>>>()?,
        })
    }

    fn from_project(project: &Project) -> Result<Self> {
        Ok(Self {
            name: project.name.clone(),
            device_settings: Vec::new(),
            chains: project
                .chains
                .iter()
                .map(ChainYaml::from_chain)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct PresetYaml {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    blocks: Vec<Value>,
}

impl PresetYaml {
    fn into_preset(self) -> Result<ChainBlocksPreset> {
        let preset_chain_id = generated_preset_chain_id(&self.id);
        Ok(ChainBlocksPreset {
            id: self.id.clone(),
            name: self.name,
            blocks: self
                .blocks
                .into_iter()
                .enumerate()
                .filter_map(|(index, block)| load_audio_block_value(block, &preset_chain_id, index))
                .collect(),
        })
    }

    fn from_chain_preset(preset: &ChainBlocksPreset) -> Result<Self> {
        Ok(Self {
            id: preset.id.clone(),
            name: preset.name.clone(),
            blocks: preset
                .blocks
                .iter()
                .map(|block| {
                    Ok(serde_yaml::to_value(AudioBlockYaml::from_audio_block(
                        block,
                    )?)?)
                })
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

fn default_yaml_bit_depth() -> u32 {
    32
}

fn default_yaml_realtime() -> bool {
    true
}

fn default_yaml_rt_priority() -> u8 {
    70
}

fn default_yaml_nperiods() -> u32 {
    2
}

#[derive(Debug, Deserialize, Serialize)]
struct DeviceSettingsYaml {
    device_id: String,
    sample_rate: u32,
    buffer_size_frames: u32,
    #[serde(default = "default_yaml_bit_depth")]
    bit_depth: u32,
    // Linux JACK tuning — always present for YAML portability. Defaults
    // preserve pre-existing behaviour on macOS/Windows.
    #[serde(default = "default_yaml_realtime")]
    realtime: bool,
    #[serde(default = "default_yaml_rt_priority")]
    rt_priority: u8,
    #[serde(default = "default_yaml_nperiods")]
    nperiods: u32,
}

impl From<DeviceSettingsYaml> for DeviceSettings {
    fn from(value: DeviceSettingsYaml) -> Self {
        Self {
            device_id: DeviceId(value.device_id),
            sample_rate: value.sample_rate,
            buffer_size_frames: value.buffer_size_frames,
            bit_depth: value.bit_depth,
            realtime: value.realtime,
            rt_priority: value.rt_priority,
            nperiods: value.nperiods,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
struct ChainInputEntryYaml {
    #[serde(default, skip_serializing)]
    name: String,
    device_id: String,
    #[serde(default)]
    mode: ChainInputMode,
    channels: Vec<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
struct ChainInputYaml {
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

fn default_io_yaml_model() -> String {
    "standard".to_string()
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
struct ChainOutputEntryYaml {
    #[serde(default, skip_serializing)]
    name: String,
    device_id: String,
    #[serde(default)]
    mode: ChainOutputMode,
    channels: Vec<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
struct ChainOutputYaml {
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
struct ChainYaml {
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
    fn into_chain(self, index: usize) -> Result<Chain> {
        let chain_id = generated_chain_id(index);
        log::debug!("deserializing chain index={}, description={:?}, instrument='{}', enabled={}", index, self.description, self.instrument, self.enabled);

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
        let has_inline_inputs = parsed_blocks.iter().any(|b| matches!(&b.kind, AudioBlockKind::Input(_)));
        let has_inline_outputs = parsed_blocks.iter().any(|b| matches!(&b.kind, AudioBlockKind::Output(_)));

        if has_inline_inputs || has_inline_outputs {
            // New format: blocks already contain I/O inline, use as-is
            let chain = Chain {
                id: chain_id.clone(),
                description: self.description,
                instrument: self.instrument,
                enabled: self.enabled,
                blocks: parsed_blocks,
            };
            return Ok(chain);
        }

        // Old format: convert separate inputs/outputs sections to blocks
        let mut input_blocks: Vec<AudioBlock> = self.inputs.into_iter().enumerate().map(|(i, inp)| {
            let entries = if !inp.entries.is_empty() {
                inp.entries.into_iter().map(|e| InputEntry {
                    device_id: DeviceId(e.device_id),
                    mode: e.mode,
                    channels: e.channels,
                }).collect()
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
        }).collect();

        let mut output_blocks: Vec<AudioBlock> = self.outputs.into_iter().enumerate().map(|(i, out)| {
            let entries = if !out.entries.is_empty() {
                out.entries.into_iter().map(|e| OutputEntry {
                    device_id: DeviceId(e.device_id),
                    mode: e.mode,
                    channels: e.channels,
                }).collect()
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
        }).collect();

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
        let mut all_blocks = Vec::with_capacity(input_blocks.len() + parsed_blocks.len() + output_blocks.len());
        all_blocks.extend(input_blocks);
        all_blocks.extend(parsed_blocks);
        all_blocks.extend(output_blocks);

        let chain = Chain {
            id: chain_id.clone(),
            description: self.description,
            instrument: self.instrument,
            enabled: self.enabled,
            blocks: all_blocks,
        };

        Ok(chain)
    }

    fn from_chain(chain: &Chain) -> Result<Self> {
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

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AudioBlockYaml {
    #[serde(rename = "preamp")]
    Preamp {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_preamp_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    #[serde(rename = "amp")]
    Amp {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_amp_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    #[serde(rename = "full_rig")]
    FullRig {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_full_rig_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Cab {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_cab_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Body {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_body_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Ir {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_ir_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    #[serde(rename = "gain")]
    Gain {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_drive_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Nam {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_nam_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Delay {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_delay_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Reverb {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_reverb_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Utility {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_utility_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Dynamics {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_dynamics_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Filter {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_filter_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Wah {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_wah_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Modulation {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_modulation_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Pitch {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_pitch_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    #[serde(rename = "vst3")]
    Vst3 {
        #[serde(default = "default_enabled")]
        enabled: bool,
        model: String,
        #[serde(default)]
        params: Value,
    },
    Select {
        #[serde(default = "default_enabled")]
        enabled: bool,
        selected: String,
        options: Vec<SelectOptionYaml>,
    },
    #[serde(rename = "input")]
    Input {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_io_yaml_model")]
        model: String,
        // Legacy: name field migrated to entry-level
        #[serde(default, skip_serializing)]
        name: String,
        #[serde(default)]
        entries: Vec<ChainInputEntryYaml>,
        // Legacy single-entry fields for backward compat
        #[serde(default, skip_serializing)]
        device_id: Option<String>,
        #[serde(default, skip_serializing)]
        mode: Option<ChainInputMode>,
        #[serde(default, skip_serializing)]
        channels: Option<Vec<usize>>,
    },
    #[serde(rename = "output")]
    Output {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_io_yaml_model")]
        model: String,
        // Legacy: name field migrated to entry-level
        #[serde(default, skip_serializing)]
        name: String,
        #[serde(default)]
        entries: Vec<ChainOutputEntryYaml>,
        // Legacy single-entry fields for backward compat
        #[serde(default, skip_serializing)]
        device_id: Option<String>,
        #[serde(default, skip_serializing)]
        mode: Option<ChainOutputMode>,
        #[serde(default, skip_serializing)]
        channels: Option<Vec<usize>>,
    },
    #[serde(rename = "insert")]
    Insert {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_io_yaml_model")]
        model: String,
        send: InsertEndpointYaml,
        #[serde(rename = "return")]
        return_: InsertEndpointYaml,
    },
}

#[derive(Debug, Deserialize, Serialize)]
struct InsertEndpointYaml {
    #[serde(default)]
    device_id: String,
    #[serde(default)]
    mode: ChainInputMode,
    #[serde(default)]
    channels: Vec<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
struct SelectOptionYaml {
    id: String,
    #[serde(flatten)]
    block: AudioBlockYaml,
}

impl AudioBlockYaml {
    fn into_audio_block(self, chain_id: &ChainId, index: usize) -> Result<AudioBlock> {
        self.into_audio_block_with_id(generated_block_id(chain_id, index))
    }

    fn into_audio_block_with_id(self, generated_id: BlockId) -> Result<AudioBlock> {
        match self {
            AudioBlockYaml::Nam {
                enabled,
                model,
                params,
            } => Ok(AudioBlock {
                id: generated_id,
                enabled,
                kind: AudioBlockKind::Nam(NamBlock {
                    model: model.clone(),
                    params: load_model_params(block_core::EFFECT_TYPE_NAM, &model, params)?,
                }),
            }),
            AudioBlockYaml::Select {
                enabled,
                selected,
                options,
            } => {
                let select_prefix = generated_id.0.clone();
                let selected_block_id = BlockId(format!("{}::{}", select_prefix, selected));
                let options = options
                    .into_iter()
                    .map(|option| {
                        let option_id = BlockId(format!("{}::{}", select_prefix, option.id));
                        option.block.into_audio_block_with_id(option_id)
                    })
                    .collect::<Result<Vec<_>>>()?;

                Ok(AudioBlock {
                    id: generated_id,
                    enabled,
                    kind: AudioBlockKind::Select(SelectBlock {
                        selected_block_id,
                        options,
                    }),
                })
            }
            AudioBlockYaml::Input {
                enabled,
                model,
                name: _,
                entries,
                device_id,
                mode,
                channels,
            } => {
                let entries = if !entries.is_empty() {
                    entries.into_iter().map(|e| InputEntry {
                        device_id: DeviceId(e.device_id),
                        mode: e.mode,
                        channels: e.channels,
                    }).collect()
                } else if let Some(device_id) = device_id {
                    vec![InputEntry {
                        device_id: DeviceId(device_id),
                        mode: mode.unwrap_or_default(),
                        channels: channels.unwrap_or_default(),
                    }]
                } else {
                    Vec::new()
                };
                Ok(AudioBlock {
                    id: generated_id,
                    enabled,
                    kind: AudioBlockKind::Input(InputBlock {
                        model,
                        entries,
                    }),
                })
            }
            AudioBlockYaml::Output {
                enabled,
                model,
                name: _,
                entries,
                device_id,
                mode,
                channels,
            } => {
                let entries = if !entries.is_empty() {
                    entries.into_iter().map(|e| OutputEntry {
                        device_id: DeviceId(e.device_id),
                        mode: e.mode,
                        channels: e.channels,
                    }).collect()
                } else if let Some(device_id) = device_id {
                    vec![OutputEntry {
                        device_id: DeviceId(device_id),
                        mode: mode.unwrap_or_default(),
                        channels: channels.unwrap_or_default(),
                    }]
                } else {
                    Vec::new()
                };
                Ok(AudioBlock {
                    id: generated_id,
                    enabled,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model,
                        entries,
                    }),
                })
            }
            AudioBlockYaml::Insert {
                enabled,
                model,
                send,
                return_,
            } => Ok(AudioBlock {
                id: generated_id,
                enabled,
                kind: AudioBlockKind::Insert(InsertBlock {
                    model,
                    send: InsertEndpoint {
                        device_id: DeviceId(send.device_id),
                        mode: send.mode,
                        channels: send.channels,
                    },
                    return_: InsertEndpoint {
                        device_id: DeviceId(return_.device_id),
                        mode: return_.mode,
                        channels: return_.channels,
                    },
                }),
            }),
            other => {
                let (effect_type, enabled, model, params) = extract_core_block_fields(other);
                Ok(AudioBlock {
                    id: generated_id,
                    enabled,
                    kind: AudioBlockKind::Core(CoreBlock {
                        effect_type: effect_type.to_string(),
                        model: model.clone(),
                        params: load_model_params(effect_type, &model, params)?,
                    }),
                })
            }
        }
    }

    fn from_audio_block(block: &AudioBlock) -> Result<Self> {
        match &block.kind {
            AudioBlockKind::Nam(stage) => Ok(Self::Nam {
                enabled: block.enabled,
                model: stage.model.clone(),
                params: parameter_set_to_yaml_value(&stage.params),
            }),
            AudioBlockKind::Core(core) => {
                let params = parameter_set_to_yaml_value(&core.params);
                let enabled = block.enabled;
                let model = core.model.clone();
                match core.effect_type.as_str() {
                    block_core::EFFECT_TYPE_PREAMP => Ok(Self::Preamp { enabled, model, params }),
                    block_core::EFFECT_TYPE_AMP => Ok(Self::Amp { enabled, model, params }),
                    block_core::EFFECT_TYPE_FULL_RIG => Ok(Self::FullRig { enabled, model, params }),
                    block_core::EFFECT_TYPE_CAB => Ok(Self::Cab { enabled, model, params }),
                    block_core::EFFECT_TYPE_BODY => Ok(Self::Body { enabled, model, params }),
                    block_core::EFFECT_TYPE_IR => Ok(Self::Ir { enabled, model, params }),
                    block_core::EFFECT_TYPE_GAIN => Ok(Self::Gain { enabled, model, params }),
                    block_core::EFFECT_TYPE_DELAY => Ok(Self::Delay { enabled, model, params }),
                    block_core::EFFECT_TYPE_REVERB => Ok(Self::Reverb { enabled, model, params }),
                    block_core::EFFECT_TYPE_UTILITY => Ok(Self::Utility { enabled, model, params }),
                    block_core::EFFECT_TYPE_DYNAMICS => Ok(Self::Dynamics { enabled, model, params }),
                    block_core::EFFECT_TYPE_FILTER => Ok(Self::Filter { enabled, model, params }),
                    block_core::EFFECT_TYPE_WAH => Ok(Self::Wah { enabled, model, params }),
                    block_core::EFFECT_TYPE_MODULATION => Ok(Self::Modulation { enabled, model, params }),
                    block_core::EFFECT_TYPE_PITCH => Ok(Self::Pitch { enabled, model, params }),
                    block_core::EFFECT_TYPE_VST3 => Ok(Self::Vst3 { enabled, model, params }),
                    other => Err(anyhow!("unsupported core block effect_type '{}'", other)),
                }
            }
            AudioBlockKind::Select(select) => {
                let selected = select
                    .selected_block_id
                    .0
                    .strip_prefix(&format!("{}::", block.id.0))
                    .unwrap_or(select.selected_block_id.0.as_str())
                    .to_string();
                let options = select
                    .options
                    .iter()
                    .enumerate()
                    .map(|(index, option)| {
                        Ok(SelectOptionYaml {
                            id: option
                                .id
                                .0
                                .strip_prefix(&format!("{}::", block.id.0))
                                .unwrap_or(option.id.0.as_str())
                                .to_string(),
                            block: AudioBlockYaml::from_audio_block(option)
                                .with_context(|| {
                                    format!(
                                        "failed to serialize select option {} for block '{}'",
                                        index,
                                        block.id.0
                                    )
                                })?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

                Ok(Self::Select {
                    enabled: block.enabled,
                    selected,
                    options,
                })
            }
            AudioBlockKind::Input(input) => Ok(Self::Input {
                enabled: block.enabled,
                model: input.model.clone(),
                name: String::new(),
                entries: input.entries.iter().map(|e| ChainInputEntryYaml {
                    name: String::new(),
                    device_id: e.device_id.0.clone(),
                    mode: e.mode,
                    channels: e.channels.clone(),
                }).collect(),
                device_id: None,
                mode: None,
                channels: None,
            }),
            AudioBlockKind::Output(output) => Ok(Self::Output {
                enabled: block.enabled,
                model: output.model.clone(),
                name: String::new(),
                entries: output.entries.iter().map(|e| ChainOutputEntryYaml {
                    name: String::new(),
                    device_id: e.device_id.0.clone(),
                    mode: e.mode,
                    channels: e.channels.clone(),
                }).collect(),
                device_id: None,
                mode: None,
                channels: None,
            }),
            AudioBlockKind::Insert(insert) => Ok(Self::Insert {
                enabled: block.enabled,
                model: insert.model.clone(),
                send: InsertEndpointYaml {
                    device_id: insert.send.device_id.0.clone(),
                    mode: insert.send.mode,
                    channels: insert.send.channels.clone(),
                },
                return_: InsertEndpointYaml {
                    device_id: insert.return_.device_id.0.clone(),
                    mode: insert.return_.mode,
                    channels: insert.return_.channels.clone(),
                },
            }),
        }
    }
}

fn load_audio_block_value(value: Value, chain_id: &ChainId, index: usize) -> Option<AudioBlock> {
    let yaml = match serde_yaml::from_value::<AudioBlockYaml>(value) {
        Ok(yaml) => yaml,
        Err(error) => {
            log::warn!(
                "ignoring unsupported or invalid block at {}:{}: {}",
                chain_id.0, index, error
            );
            eprintln!(
                "ignoring unsupported or invalid block at {}:{}: {}",
                chain_id.0, index, error
            );
            return None;
        }
    };

    match yaml.into_audio_block(chain_id, index) {
        Ok(block) => {
            log::debug!("loaded block at {}:{}", chain_id.0, index);
            Some(block)
        }
        Err(error) => {
            log::warn!(
                "ignoring unsupported or invalid block at {}:{}: {}",
                chain_id.0, index, error
            );
            eprintln!(
                "ignoring unsupported or invalid block at {}:{}: {}",
                chain_id.0, index, error
            );
            None
        }
    }
}

fn load_model_params(effect_type: &str, model: &str, raw_params: Value) -> Result<ParameterSet> {
    let flattened = flatten_parameter_set(raw_params)?;
    normalize_block_params(effect_type, model, flattened).map_err(anyhow::Error::msg)
}

fn extract_core_block_fields(yaml: AudioBlockYaml) -> (&'static str, bool, String, Value) {
    match yaml {
        AudioBlockYaml::Preamp { enabled, model, params } => (block_core::EFFECT_TYPE_PREAMP, enabled, model, params),
        AudioBlockYaml::Amp { enabled, model, params } => (block_core::EFFECT_TYPE_AMP, enabled, model, params),
        AudioBlockYaml::FullRig { enabled, model, params } => (block_core::EFFECT_TYPE_FULL_RIG, enabled, model, params),
        AudioBlockYaml::Cab { enabled, model, params } => (block_core::EFFECT_TYPE_CAB, enabled, model, params),
        AudioBlockYaml::Body { enabled, model, params } => (block_core::EFFECT_TYPE_BODY, enabled, model, params),
        AudioBlockYaml::Ir { enabled, model, params } => (block_core::EFFECT_TYPE_IR, enabled, model, params),
        AudioBlockYaml::Gain { enabled, model, params } => (block_core::EFFECT_TYPE_GAIN, enabled, model, params),
        AudioBlockYaml::Delay { enabled, model, params } => (block_core::EFFECT_TYPE_DELAY, enabled, model, params),
        AudioBlockYaml::Reverb { enabled, model, params } => (block_core::EFFECT_TYPE_REVERB, enabled, model, params),
        AudioBlockYaml::Utility { enabled, model, params } => (block_core::EFFECT_TYPE_UTILITY, enabled, model, params),
        AudioBlockYaml::Dynamics { enabled, model, params } => (block_core::EFFECT_TYPE_DYNAMICS, enabled, model, params),
        AudioBlockYaml::Filter { enabled, model, params } => (block_core::EFFECT_TYPE_FILTER, enabled, model, params),
        AudioBlockYaml::Wah { enabled, model, params } => (block_core::EFFECT_TYPE_WAH, enabled, model, params),
        AudioBlockYaml::Modulation { enabled, model, params } => (block_core::EFFECT_TYPE_MODULATION, enabled, model, params),
        AudioBlockYaml::Pitch { enabled, model, params } => (block_core::EFFECT_TYPE_PITCH, enabled, model, params),
        AudioBlockYaml::Vst3 { enabled, model, params } => (block_core::EFFECT_TYPE_VST3, enabled, model, params),
        AudioBlockYaml::Nam { enabled, model, params } => (block_core::EFFECT_TYPE_NAM, enabled, model, params),
        AudioBlockYaml::Select { .. } => unreachable!("Select handled before extract_core_block_fields"),
        AudioBlockYaml::Input { .. } => unreachable!("Input handled before extract_core_block_fields"),
        AudioBlockYaml::Output { .. } => unreachable!("Output handled before extract_core_block_fields"),
        AudioBlockYaml::Insert { .. } => unreachable!("Insert handled before extract_core_block_fields"),
    }
}

fn flatten_parameter_set(value: Value) -> Result<ParameterSet> {
    let mut params = ParameterSet::default();
    match value {
        Value::Null => Ok(params),
        Value::Mapping(mapping) => {
            for (key, value) in mapping {
                let key = yaml_key_to_string(key)?;
                flatten_parameter_value(&mut params, &key, value)?;
            }
            Ok(params)
        }
        other => Err(anyhow!("params must be a mapping, got {:?}", other)),
    }
}

fn flatten_parameter_value(params: &mut ParameterSet, path: &str, value: Value) -> Result<()> {
    match value {
        Value::Mapping(mapping) => {
            for (key, nested_value) in mapping {
                let key = yaml_key_to_string(key)?;
                let nested_path = format!("{}.{}", path, key);
                flatten_parameter_value(params, &nested_path, nested_value)?;
            }
            Ok(())
        }
        scalar => {
            params.insert(path.to_string(), yaml_scalar_to_parameter_value(scalar)?);
            Ok(())
        }
    }
}

fn parameter_set_to_yaml_value(params: &ParameterSet) -> Value {
    let mut root = serde_yaml::Mapping::new();
    for (path, value) in &params.values {
        let parts = path.split('.').collect::<Vec<_>>();
        insert_yaml_value(&mut root, &parts, parameter_value_to_yaml(value));
    }
    Value::Mapping(root)
}

fn insert_yaml_value(mapping: &mut serde_yaml::Mapping, path: &[&str], value: Value) {
    if path.is_empty() {
        return;
    }
    let key = Value::String(path[0].to_string());
    if path.len() == 1 {
        mapping.insert(key, value);
        return;
    }

    if !matches!(mapping.get(&key), Some(Value::Mapping(_))) {
        mapping.insert(key.clone(), Value::Mapping(serde_yaml::Mapping::new()));
    }

    if let Some(Value::Mapping(child)) = mapping.get_mut(&key) {
        insert_yaml_value(child, &path[1..], value);
    }
}

fn parameter_value_to_yaml(value: &ParameterValue) -> Value {
    match value {
        ParameterValue::Null => Value::Null,
        ParameterValue::Bool(value) => Value::Bool(*value),
        ParameterValue::Int(value) => serde_yaml::to_value(value).unwrap_or(Value::Null),
        ParameterValue::Float(value) => serde_yaml::to_value(value).unwrap_or(Value::Null),
        ParameterValue::String(value) => Value::String(value.clone()),
    }
}

fn yaml_key_to_string(value: Value) -> Result<String> {
    match value {
        Value::String(value) => Ok(value),
        other => Err(anyhow!("yaml object keys must be strings, got {:?}", other)),
    }
}

fn yaml_scalar_to_parameter_value(value: Value) -> Result<ParameterValue> {
    match value {
        Value::Null => Ok(ParameterValue::Null),
        Value::Bool(value) => Ok(ParameterValue::Bool(value)),
        Value::Number(value) => {
            if let Some(number) = value.as_i64() {
                Ok(ParameterValue::Int(number))
            } else if let Some(number) = value.as_f64() {
                Ok(ParameterValue::Float(number as f32))
            } else {
                Err(anyhow!("unsupported yaml number '{}'", value))
            }
        }
        Value::String(value) => Ok(ParameterValue::String(value)),
        Value::Sequence(_) | Value::Mapping(_) | Value::Tagged(_) => {
            Err(anyhow!("unsupported yaml value in params"))
        }
    }
}

fn generated_block_id(chain_id: &ChainId, index: usize) -> BlockId {
    BlockId(format!("{}:block:{}", chain_id.0, index))
}

fn generated_chain_id(index: usize) -> ChainId {
    ChainId(format!("chain:{}", index))
}

fn generated_preset_chain_id(preset_id: &str) -> ChainId {
    ChainId(format!("preset:{}", preset_id))
}

fn default_delay_model() -> String {
    block_delay::supported_models()
        .first()
        .expect("block-delay must expose at least one model")
        .to_string()
}

fn default_nam_model() -> String {
    block_nam::supported_models()
        .first()
        .expect("block-nam must expose at least one model")
        .to_string()
}

fn default_preamp_model() -> String {
    block_preamp::supported_models()
        .first()
        .expect("block-preamp must expose at least one model")
        .to_string()
}

fn default_amp_model() -> String {
    block_amp::supported_models()
        .first()
        .expect("block-amp must expose at least one model")
        .to_string()
}

fn default_full_rig_model() -> String {
    block_full_rig::supported_models()
        .first()
        .expect("block-full-rig must expose at least one model")
        .to_string()
}

fn default_cab_model() -> String {
    block_cab::supported_models()
        .first()
        .expect("block-cab must expose at least one model")
        .to_string()
}

fn default_body_model() -> String {
    block_body::supported_models()
        .first()
        .expect("block-body must expose at least one model")
        .to_string()
}

fn default_drive_model() -> String {
    block_gain::supported_models()
        .first()
        .expect("block-gain must expose at least one model")
        .to_string()
}

fn default_reverb_model() -> String {
    block_reverb::supported_models()
        .first()
        .expect("block-reverb must expose at least one model")
        .to_string()
}

fn default_utility_model() -> String {
    block_util::supported_models()
        .first()
        .expect("block-util must expose at least one model")
        .to_string()
}

fn default_dynamics_model() -> String {
    block_dyn::supported_models()
        .first()
        .expect("block-dyn must expose at least one model")
        .to_string()
}

fn default_filter_model() -> String {
    block_filter::supported_models()
        .first()
        .expect("block-filter must expose at least one model")
        .to_string()
}

fn default_ir_model() -> String {
    block_ir::supported_models()
        .first()
        .expect("block-ir must expose at least one model")
        .to_string()
}

fn default_wah_model() -> String {
    block_wah::supported_models()
        .first()
        .expect("block-wah must expose at least one model")
        .to_string()
}

fn default_modulation_model() -> String {
    block_mod::supported_models()
        .first()
        .expect("block-mod must expose at least one model")
        .to_string()
}

fn default_pitch_model() -> String {
    block_pitch::supported_models()
        .first()
        .expect("block-pitch must expose at least one model")
        .to_string()
}

const fn default_enabled() -> bool {
    true
}

fn default_instrument() -> String {
    block_core::DEFAULT_INSTRUMENT.to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        load_chain_preset_file, save_chain_preset_file, ChainBlocksPreset, YamlProjectRepository,
    };
    use domain::ids::{BlockId, DeviceId, ChainId};
    use project::block::{
        AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, OutputBlock, OutputEntry, SelectBlock,
    };
    use project::param::ParameterSet;
    use project::project::Project;
    use project::chain::{Chain, ChainInputMode, ChainOutputMode};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn save_project_creates_yaml_that_roundtrips_basic_project() {
        let temp_dir = tempdir().expect("temp dir should be created");
        let project_path = temp_dir.path().join("project.yaml");
        let repository = YamlProjectRepository {
            path: project_path.clone(),
        };
        let original = Project {
            name: Some("Test Project".into()),
            device_settings: Vec::new(),
            chains: vec![Chain {
                id: ChainId("chain:0".into()),
                description: Some("Guitar 1".into()),
                instrument: "electric_guitar".to_string(),
                enabled: true,
                blocks: vec![
                    AudioBlock {
                        id: BlockId("chain:0:input:0".into()),
                        enabled: true,
                        kind: AudioBlockKind::Input(InputBlock {
                            model: "standard".to_string(),
                            entries: vec![InputEntry {
                                device_id: DeviceId("input-device".into()),
                                mode: ChainInputMode::Mono,
                                channels: vec![0],
                            }],
                        }),
                    },
                    AudioBlock {
                        id: BlockId("chain:0:output:0".into()),
                        enabled: true,
                        kind: AudioBlockKind::Output(OutputBlock {
                            model: "standard".to_string(),
                            entries: vec![OutputEntry {
                                device_id: DeviceId("output-device".into()),
                                mode: ChainOutputMode::Stereo,
                                channels: vec![0, 1],
                            }],
                        }),
                    },
                ],
            }],
        };

        repository
            .save_project(&original)
            .expect("project save should succeed");

        assert!(project_path.exists(), "project yaml should be written");

        let loaded = repository
            .load_current_project()
            .expect("saved project should load");

        assert_eq!(loaded.name, original.name);
        assert_eq!(loaded.chains.len(), 1);
        assert_eq!(loaded.chains[0].description, original.chains[0].description);
        let loaded_inputs = loaded.chains[0].input_blocks();
        assert_eq!(loaded_inputs.len(), 1);
        assert_eq!(loaded_inputs[0].1.entries[0].device_id, DeviceId("input-device".into()));
        assert_eq!(loaded_inputs[0].1.entries[0].channels, vec![0]);
        let loaded_outputs = loaded.chains[0].output_blocks();
        assert_eq!(loaded_outputs.len(), 1);
        assert_eq!(loaded_outputs[0].1.entries[0].device_id, DeviceId("output-device".into()));
        assert_eq!(loaded_outputs[0].1.entries[0].channels, vec![0, 1]);
    }

    #[test]
    fn load_project_migrates_legacy_io_format() {
        let temp_dir = tempdir().expect("temp dir should be created");
        let project_path = temp_dir.path().join("project.yaml");
        fs::write(
            &project_path,
            r#"
chains:
  - enabled: true
    input_device_id: legacy-input
    input_channels: [0]
    output_device_id: legacy-output
    output_channels: [0, 1]
    blocks: []
"#,
        )
        .expect("project yaml should be written");

        let repository = YamlProjectRepository { path: project_path };
        let project = repository
            .load_current_project()
            .expect("legacy project should load with migration");

        assert_eq!(project.chains.len(), 1);
        let inputs = project.chains[0].input_blocks();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].1.entries[0].device_id, DeviceId("legacy-input".into()));
        assert_eq!(inputs[0].1.entries[0].channels, vec![0]);
        let outputs = project.chains[0].output_blocks();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].1.entries[0].device_id, DeviceId("legacy-output".into()));
        assert_eq!(outputs[0].1.entries[0].channels, vec![0, 1]);
        assert_eq!(outputs[0].1.entries[0].mode, ChainOutputMode::Stereo);
    }

    #[test]
    fn load_project_ignores_removed_or_invalid_blocks() {
        let temp_dir = tempdir().expect("temp dir should be created");
        let project_path = temp_dir.path().join("project.yaml");
        let valid_delay_model = block_delay::supported_models()
            .first()
            .expect("block-delay must expose at least one model");
        fs::write(
            &project_path,
            format!(
                r#"
chains:
  - enabled: true
    input_device_id: input-device
    input_channels: [0]
    output_device_id: output-device
    output_channels: [0]
    blocks:
      - type: core_nam
        enabled: true
        model_id: legacy
      - type: delay
        enabled: true
        model: {valid_delay_model}
        params:
          time_ms: 200
          feedback: 50
          mix: 30
"#,
            ),
        )
        .expect("project yaml should be written");

        let repository = YamlProjectRepository { path: project_path };
        let project = repository
            .load_current_project()
            .expect("project should load while skipping invalid blocks");

        assert_eq!(project.chains.len(), 1);
        // 1 InputBlock + 1 valid delay block + 1 OutputBlock = 3 total
        let audio_blocks: Vec<_> = project.chains[0].blocks.iter()
            .filter(|b| !matches!(&b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)))
            .collect();
        assert_eq!(audio_blocks.len(), 1);
        assert_eq!(
            audio_blocks[0]
                .model_ref()
                .expect("remaining block should expose model")
                .model,
            *valid_delay_model
        );
    }

    #[test]
    fn load_preset_ignores_unknown_models() {
        let temp_dir = tempdir().expect("temp dir should be created");
        let preset_path: PathBuf = temp_dir.path().join("example.yaml");
        let valid_delay_model = block_delay::supported_models()
            .first()
            .expect("block-delay must expose at least one model");
        fs::write(
            &preset_path,
            format!(
                r#"
id: example
blocks:
  - type: delay
    model: deleted_model
    params:
      time_ms: 200
      feedback: 50
      mix: 30
  - type: delay
    model: {valid_delay_model}
    params:
      time_ms: 210
      feedback: 40
      mix: 25
"#,
            ),
        )
        .expect("preset yaml should be written");

        let preset = load_chain_preset_file(&preset_path)
            .expect("preset should load while skipping invalid blocks");

        assert_eq!(preset.blocks.len(), 1);
        assert_eq!(
            preset.blocks[0]
                .model_ref()
                .expect("remaining block should expose model")
                .model,
            *valid_delay_model
        );
    }

    #[test]
    fn load_project_supports_generic_select_options() {
        let temp_dir = tempdir().expect("temp dir should be created");
        let project_path = temp_dir.path().join("project.yaml");
        let delay_models = block_delay::supported_models();
        let first_model = delay_models
            .first()
            .expect("block-delay must expose at least one model");
        let second_model = delay_models
            .get(1)
            .unwrap_or(first_model);

        fs::write(
            &project_path,
            format!(
                r#"
chains:
  - enabled: true
    input_device_id: input-device
    input_channels: [0]
    output_device_id: output-device
    output_channels: [0]
    blocks:
      - type: select
        enabled: true
        selected: delay_b
        options:
          - id: delay_a
            type: delay
            model: {first_model}
            params:
              time_ms: 120
              feedback: 20
              mix: 30
          - id: delay_b
            type: delay
            model: {second_model}
            params:
              time_ms: 240
              feedback: 40
              mix: 25
"#,
            ),
        )
        .expect("project yaml should be written");

        let repository = YamlProjectRepository { path: project_path };
        let project = repository
            .load_current_project()
            .expect("project should load generic select blocks");

        // Find the first non-I/O block (should be the select block)
        let audio_block = project.chains[0].blocks.iter()
            .find(|b| !matches!(&b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)))
            .expect("should have at least one audio block");
        let select = match &audio_block.kind {
            AudioBlockKind::Select(select) => select,
            other => panic!("expected select block, got {:?}", other),
        };
        assert_eq!(select.options.len(), 2);
        assert_eq!(select.selected_block_id.0, "chain:0:block:0::delay_b");
    }

    #[test]
    fn preset_roundtrips_generic_select_options() {
        let temp_dir = tempdir().expect("temp dir should be created");
        let preset_path: PathBuf = temp_dir.path().join("select.yaml");
        let delay_models = block_delay::supported_models();
        let first_model = delay_models
            .first()
            .expect("block-delay must expose at least one model");
        let second_model = delay_models
            .get(1)
            .unwrap_or(first_model);
        let preset = ChainBlocksPreset {
            id: "select".into(),
            name: Some("Delay Select".into()),
            blocks: vec![AudioBlock {
                id: BlockId("preset:select:block:0".into()),
                enabled: true,
                kind: AudioBlockKind::Select(SelectBlock {
                    selected_block_id: BlockId("preset:select:block:0::delay_b".into()),
                    options: vec![
                        delay_block("preset:select:block:0::delay_a", first_model, 120.0),
                        delay_block("preset:select:block:0::delay_b", second_model, 240.0),
                    ],
                }),
            }],
        };

        save_chain_preset_file(&preset_path, &preset).expect("preset save should succeed");
        let raw = fs::read_to_string(&preset_path).expect("saved preset should be readable");
        assert!(raw.contains("type: select"));
        assert!(raw.contains("- id: delay_a"));
        assert!(raw.contains("- id: delay_b"));

        let loaded = load_chain_preset_file(&preset_path).expect("preset should reload");
        let select = match &loaded.blocks[0].kind {
            AudioBlockKind::Select(select) => select,
            other => panic!("expected select block, got {:?}", other),
        };
        assert_eq!(select.selected_block_id.0, "preset:select:block:0::delay_b");
        assert_eq!(select.options.len(), 2);
    }

    fn delay_block(id: impl Into<String>, model: &str, time_ms: f32) -> AudioBlock {
        let schema =
            project::block::schema_for_block_model("delay", model).expect("delay schema exists");
        let mut params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("delay defaults should normalize");
        params.insert("time_ms", domain::value_objects::ParameterValue::Float(time_ms));
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

    #[test]
    fn insert_block_yaml_roundtrip() {
        use project::block::{InsertBlock, InsertEndpoint};
        let block = AudioBlock {
            id: BlockId("chain:0:block:1".into()),
            enabled: true,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".to_string(),
                send: InsertEndpoint {
                    device_id: DeviceId("mk300-out".into()),
                    mode: ChainInputMode::Stereo,
                    channels: vec![0, 1],
                },
                return_: InsertEndpoint {
                    device_id: DeviceId("mk300-in".into()),
                    mode: ChainInputMode::Stereo,
                    channels: vec![0, 1],
                },
            }),
        };
        let yaml = super::AudioBlockYaml::from_audio_block(&block).expect("to yaml");
        let value = serde_yaml::to_value(&yaml).expect("serialize");
        let parsed: super::AudioBlockYaml = serde_yaml::from_value(value).expect("deserialize");
        let chain_id = ChainId("chain:0".to_string());
        let restored = parsed.into_audio_block(&chain_id, 1).expect("into block");
        assert!(matches!(&restored.kind, AudioBlockKind::Insert(ib) if ib.send.device_id.0 == "mk300-out"));
        assert!(matches!(&restored.kind, AudioBlockKind::Insert(ib) if ib.return_.device_id.0 == "mk300-in"));
        assert!(matches!(&restored.kind, AudioBlockKind::Insert(ib) if ib.send.channels == vec![0, 1]));
        assert!(matches!(&restored.kind, AudioBlockKind::Insert(ib) if ib.return_.channels == vec![0, 1]));
        assert!(matches!(&restored.kind, AudioBlockKind::Insert(ib) if ib.send.mode == ChainInputMode::Stereo));
    }

    #[test]
    fn disabled_insert_block_yaml_roundtrip() {
        use project::block::{InsertBlock, InsertEndpoint};
        let block = AudioBlock {
            id: BlockId("chain:0:block:2".into()),
            enabled: false,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".to_string(),
                send: InsertEndpoint {
                    device_id: DeviceId(String::new()),
                    mode: ChainInputMode::Mono,
                    channels: Vec::new(),
                },
                return_: InsertEndpoint {
                    device_id: DeviceId(String::new()),
                    mode: ChainInputMode::Mono,
                    channels: Vec::new(),
                },
            }),
        };
        let yaml = super::AudioBlockYaml::from_audio_block(&block).expect("to yaml");
        let value = serde_yaml::to_value(&yaml).expect("serialize");
        let parsed: super::AudioBlockYaml = serde_yaml::from_value(value).expect("deserialize");
        let chain_id = ChainId("chain:0".to_string());
        let restored = parsed.into_audio_block(&chain_id, 2).expect("into block");
        assert!(!restored.enabled);
        assert!(matches!(&restored.kind, AudioBlockKind::Insert(_)));
    }

    // ─── Helper: build a CoreBlock AudioBlock for a given effect type + model ───

    fn core_block(
        id: &str,
        effect_type: &str,
        model: &str,
        param_overrides: Vec<(&str, domain::value_objects::ParameterValue)>,
    ) -> AudioBlock {
        let schema = project::block::schema_for_block_model(effect_type, model)
            .expect("schema should exist");
        let mut params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize");
        for (k, v) in param_overrides {
            params.insert(k, v);
        }
        AudioBlock {
            id: BlockId(id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: effect_type.to_string(),
                model: model.to_string(),
                params,
            }),
        }
    }

    fn first_model<'a>(models: &'a [&'a str]) -> &'a str {
        models.first().expect("block crate must expose at least one model")
    }

    fn assert_core_roundtrip(effect_type: &str, model: &str) {
        let block = core_block(
            "chain:0:block:0",
            effect_type,
            model,
            Vec::new(),
        );
        let yaml = super::AudioBlockYaml::from_audio_block(&block).expect("to yaml");
        let value = serde_yaml::to_value(&yaml).expect("serialize");
        let parsed: super::AudioBlockYaml =
            serde_yaml::from_value(value).expect("deserialize");
        let chain_id = ChainId("chain:0".to_string());
        let restored = parsed.into_audio_block(&chain_id, 0).expect("into block");
        match &restored.kind {
            AudioBlockKind::Core(core) => {
                assert_eq!(core.effect_type, effect_type);
                assert_eq!(core.model, model);
            }
            other => panic!(
                "expected Core block for effect_type={}, got {:?}",
                effect_type, other
            ),
        }
    }

    // ─── Roundtrip tests for all core block types ───

    #[test]
    fn roundtrip_preamp_block_preserves_type_and_model() {
        assert_core_roundtrip("preamp", first_model(block_preamp::supported_models()));
    }

    #[test]
    fn roundtrip_amp_block_preserves_type_and_model() {
        assert_core_roundtrip("amp", first_model(block_amp::supported_models()));
    }

    #[test]
    fn roundtrip_cab_block_preserves_type_and_model() {
        assert_core_roundtrip("cab", first_model(block_cab::supported_models()));
    }

    #[test]
    fn roundtrip_body_block_preserves_type_and_model() {
        assert_core_roundtrip("body", first_model(block_body::supported_models()));
    }

    #[test]
    fn roundtrip_gain_block_preserves_type_and_model() {
        assert_core_roundtrip("gain", first_model(block_gain::supported_models()));
    }

    #[test]
    fn roundtrip_delay_block_preserves_type_and_model() {
        assert_core_roundtrip("delay", first_model(block_delay::supported_models()));
    }

    #[test]
    fn roundtrip_reverb_block_preserves_type_and_model() {
        assert_core_roundtrip("reverb", first_model(block_reverb::supported_models()));
    }

    #[test]
    fn roundtrip_dynamics_block_preserves_type_and_model() {
        assert_core_roundtrip("dynamics", first_model(block_dyn::supported_models()));
    }

    #[test]
    fn roundtrip_filter_block_preserves_type_and_model() {
        assert_core_roundtrip("filter", first_model(block_filter::supported_models()));
    }

    #[test]
    fn roundtrip_wah_block_preserves_type_and_model() {
        assert_core_roundtrip("wah", first_model(block_wah::supported_models()));
    }

    #[test]
    fn roundtrip_modulation_block_preserves_type_and_model() {
        assert_core_roundtrip("modulation", first_model(block_mod::supported_models()));
    }

    #[test]
    fn roundtrip_pitch_block_preserves_type_and_model() {
        assert_core_roundtrip("pitch", first_model(block_pitch::supported_models()));
    }

    #[test]
    fn roundtrip_utility_block_preserves_type_and_model() {
        assert_core_roundtrip("utility", first_model(block_util::supported_models()));
    }

    #[test]
    fn roundtrip_ir_block_serializes_and_deserializes_yaml() {
        use domain::value_objects::ParameterValue;
        // IR normalization validates the file exists on disk, so we only test
        // the YAML serialization layer (from_audio_block -> to_value -> back).
        let model = first_model(block_ir::supported_models());
        let mut params = ParameterSet::default();
        params.insert("file", ParameterValue::String("/some/path.wav".into()));
        let block = AudioBlock {
            id: BlockId("chain:0:block:0".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "ir".to_string(),
                model: model.to_string(),
                params,
            }),
        };
        let yaml = super::AudioBlockYaml::from_audio_block(&block).expect("to yaml");
        let value = serde_yaml::to_value(&yaml).expect("serialize");
        // Verify the serialized YAML has the correct type and model
        let yaml_str = serde_yaml::to_string(&value).expect("to string");
        assert!(yaml_str.contains("type: ir"));
        assert!(yaml_str.contains(&format!("model: {}", model)));
        assert!(yaml_str.contains("/some/path.wav"));
    }

    #[test]
    fn roundtrip_full_rig_block_preserves_type_and_model() {
        let models = block_full_rig::supported_models();
        if models.is_empty() {
            // full_rig has no models yet (reserved for future use), skip
            return;
        }
        assert_core_roundtrip("full_rig", first_model(models));
    }

    // ─── Empty project ───

    #[test]
    fn serialize_empty_project_roundtrips() {
        let project = Project {
            name: None,
            device_settings: Vec::new(),
            chains: Vec::new(),
        };
        let yaml_str = super::serialize_project(&project).expect("serialize should succeed");
        let dto: super::ProjectYaml =
            serde_yaml::from_str(&yaml_str).expect("should parse back");
        let loaded = dto.into_project().expect("should convert");
        assert!(loaded.name.is_none());
        assert!(loaded.chains.is_empty());
        assert!(loaded.device_settings.is_empty());
    }

    // ─── Chain with only input + output (no effect blocks) ───

    #[test]
    fn chain_with_only_io_blocks_roundtrips() {
        let temp_dir = tempdir().expect("temp dir");
        let path = temp_dir.path().join("io_only.yaml");
        let repo = YamlProjectRepository { path: path.clone() };
        let project = Project {
            name: Some("IO Only".into()),
            device_settings: Vec::new(),
            chains: vec![Chain {
                id: ChainId("chain:0".into()),
                description: Some("Empty chain".into()),
                instrument: "electric_guitar".to_string(),
                enabled: false,
                blocks: vec![
                    AudioBlock {
                        id: BlockId("chain:0:input:0".into()),
                        enabled: true,
                        kind: AudioBlockKind::Input(InputBlock {
                            model: "standard".to_string(),
                            entries: vec![InputEntry {
                                device_id: DeviceId("dev-in".into()),
                                mode: ChainInputMode::Mono,
                                channels: vec![0],
                            }],
                        }),
                    },
                    AudioBlock {
                        id: BlockId("chain:0:output:0".into()),
                        enabled: true,
                        kind: AudioBlockKind::Output(OutputBlock {
                            model: "standard".to_string(),
                            entries: vec![OutputEntry {
                                device_id: DeviceId("dev-out".into()),
                                mode: ChainOutputMode::Mono,
                                channels: vec![0],
                            }],
                        }),
                    },
                ],
            }],
        };
        repo.save_project(&project).expect("save");
        let loaded = repo.load_current_project().expect("load");
        assert_eq!(loaded.chains[0].blocks.len(), 2);
        assert!(matches!(&loaded.chains[0].blocks[0].kind, AudioBlockKind::Input(_)));
        assert!(matches!(&loaded.chains[0].blocks[1].kind, AudioBlockKind::Output(_)));
        // No effect blocks
        let effect_blocks: Vec<_> = loaded.chains[0].blocks.iter()
            .filter(|b| !matches!(&b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)))
            .collect();
        assert!(effect_blocks.is_empty());
    }

    // ─── Parameter boundary values ───

    #[test]
    fn parameter_boundary_zero_value_roundtrips() {
        use domain::value_objects::ParameterValue;
        let block = core_block(
            "chain:0:block:0",
            "delay",
            first_model(block_delay::supported_models()),
            vec![("time_ms", ParameterValue::Float(0.0))],
        );
        let yaml = super::AudioBlockYaml::from_audio_block(&block).expect("to yaml");
        let value = serde_yaml::to_value(&yaml).expect("serialize");
        let parsed: super::AudioBlockYaml = serde_yaml::from_value(value).expect("deserialize");
        let chain_id = ChainId("chain:0".to_string());
        let restored = parsed.into_audio_block(&chain_id, 0).expect("into block");
        if let AudioBlockKind::Core(core) = &restored.kind {
            let time = core.params.get("time_ms");
            assert!(time.is_some(), "time_ms should be present");
            match time.unwrap() {
                domain::value_objects::ParameterValue::Float(v) => assert_eq!(*v, 0.0),
                domain::value_objects::ParameterValue::Int(v) => assert_eq!(*v, 0),
                other => panic!("unexpected type for time_ms: {:?}", other),
            }
        } else {
            panic!("expected Core block");
        }
    }

    #[test]
    fn parameter_boundary_max_value_roundtrips() {
        use domain::value_objects::ParameterValue;
        let block = core_block(
            "chain:0:block:0",
            "delay",
            first_model(block_delay::supported_models()),
            vec![("mix", ParameterValue::Float(100.0))],
        );
        let yaml = super::AudioBlockYaml::from_audio_block(&block).expect("to yaml");
        let value = serde_yaml::to_value(&yaml).expect("serialize");
        let parsed: super::AudioBlockYaml = serde_yaml::from_value(value).expect("deserialize");
        let chain_id = ChainId("chain:0".to_string());
        let restored = parsed.into_audio_block(&chain_id, 0).expect("into block");
        if let AudioBlockKind::Core(core) = &restored.kind {
            let mix = core.params.get("mix");
            assert!(mix.is_some());
            match mix.unwrap() {
                domain::value_objects::ParameterValue::Float(v) => assert_eq!(*v, 100.0),
                other => panic!("unexpected type for mix: {:?}", other),
            }
        } else {
            panic!("expected Core block");
        }
    }

    // ─── Legacy format migration: inputs/outputs sections ───

    #[test]
    fn load_project_migrates_legacy_inputs_outputs_sections() {
        let temp_dir = tempdir().expect("temp dir");
        let project_path = temp_dir.path().join("legacy_sections.yaml");
        fs::write(
            &project_path,
            r#"
chains:
  - description: Legacy chain
    instrument: electric_guitar
    inputs:
      - device_id: legacy-mic
        mode: mono
        channels: [0]
    outputs:
      - device_id: legacy-speaker
        mode: stereo
        channels: [0, 1]
    blocks: []
"#,
        )
        .expect("write");
        let repo = YamlProjectRepository { path: project_path };
        let project = repo.load_current_project().expect("load");
        assert_eq!(project.chains.len(), 1);
        let inputs = project.chains[0].input_blocks();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].1.entries[0].device_id, DeviceId("legacy-mic".into()));
        let outputs = project.chains[0].output_blocks();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].1.entries[0].device_id, DeviceId("legacy-speaker".into()));
        assert_eq!(outputs[0].1.entries[0].mode, ChainOutputMode::Stereo);
    }

    #[test]
    fn load_project_migrates_legacy_input_with_entries_format() {
        let temp_dir = tempdir().expect("temp dir");
        let project_path = temp_dir.path().join("legacy_entries.yaml");
        fs::write(
            &project_path,
            r#"
chains:
  - description: Entries chain
    instrument: bass
    inputs:
      - entries:
          - name: Bass Input
            device_id: bass-interface
            mode: mono
            channels: [1]
    outputs:
      - entries:
          - name: Bass Output
            device_id: bass-monitor
            mode: mono
            channels: [0]
    blocks: []
"#,
        )
        .expect("write");
        let repo = YamlProjectRepository { path: project_path };
        let project = repo.load_current_project().expect("load");
        let inputs = project.chains[0].input_blocks();
        assert_eq!(inputs[0].1.entries[0].device_id, DeviceId("bass-interface".into()));
        assert_eq!(inputs[0].1.entries[0].channels, vec![1]);
    }

    // ─── flatten_parameter_set edge cases ───

    #[test]
    fn flatten_parameter_set_null_returns_empty() {
        let result = super::flatten_parameter_set(serde_yaml::Value::Null)
            .expect("null should flatten to empty");
        assert!(result.values.is_empty());
    }

    #[test]
    fn flatten_parameter_set_nested_mapping_flattens_with_dot_notation() {
        use serde_yaml::Value;
        let yaml: Value = serde_yaml::from_str(
            r#"
eq:
  low: 50.0
  high: 80.0
volume: 75.0
"#,
        )
        .expect("parse");
        let result = super::flatten_parameter_set(yaml).expect("flatten");
        assert!(result.values.contains_key("eq.low"));
        assert!(result.values.contains_key("eq.high"));
        assert!(result.values.contains_key("volume"));
    }

    #[test]
    fn flatten_parameter_set_non_mapping_returns_error() {
        use serde_yaml::Value;
        let yaml = Value::String("not a mapping".into());
        let result = super::flatten_parameter_set(yaml);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("params must be a mapping"));
    }

    #[test]
    fn flatten_parameter_set_bool_and_string_values() {
        use serde_yaml::Value;
        let yaml: Value = serde_yaml::from_str(
            r#"
mute: true
mode: clean
"#,
        )
        .expect("parse");
        let result = super::flatten_parameter_set(yaml).expect("flatten");
        assert_eq!(
            result.values.get("mute"),
            Some(&domain::value_objects::ParameterValue::Bool(true))
        );
        assert_eq!(
            result.values.get("mode"),
            Some(&domain::value_objects::ParameterValue::String("clean".into()))
        );
    }

    // ─── parameter_set_to_yaml_value edge cases ───

    #[test]
    fn parameter_set_to_yaml_value_empty_returns_empty_mapping() {
        let params = ParameterSet::default();
        let value = super::parameter_set_to_yaml_value(&params);
        match value {
            serde_yaml::Value::Mapping(m) => assert!(m.is_empty()),
            other => panic!("expected empty mapping, got {:?}", other),
        }
    }

    #[test]
    fn parameter_set_to_yaml_value_nested_keys_produce_nested_mapping() {
        use domain::value_objects::ParameterValue;
        let mut params = ParameterSet::default();
        params.insert("eq.low", ParameterValue::Float(30.0));
        params.insert("eq.high", ParameterValue::Float(70.0));
        params.insert("volume", ParameterValue::Float(50.0));

        let value = super::parameter_set_to_yaml_value(&params);
        let yaml_str = serde_yaml::to_string(&value).expect("serialize");
        assert!(yaml_str.contains("eq:"));
        assert!(yaml_str.contains("low:"));
        assert!(yaml_str.contains("high:"));
        assert!(yaml_str.contains("volume:"));
    }

    #[test]
    fn parameter_set_to_yaml_value_null_bool_int_string() {
        use domain::value_objects::ParameterValue;
        let mut params = ParameterSet::default();
        params.insert("a_null", ParameterValue::Null);
        params.insert("a_bool", ParameterValue::Bool(false));
        params.insert("an_int", ParameterValue::Int(42));
        params.insert("a_str", ParameterValue::String("hello".into()));

        let value = super::parameter_set_to_yaml_value(&params);
        // Roundtrip back
        let restored = super::flatten_parameter_set(value).expect("flatten roundtrip");
        assert_eq!(restored.values.get("a_null"), Some(&ParameterValue::Null));
        assert_eq!(restored.values.get("a_bool"), Some(&ParameterValue::Bool(false)));
        assert_eq!(restored.values.get("an_int"), Some(&ParameterValue::Int(42)));
        assert_eq!(restored.values.get("a_str"), Some(&ParameterValue::String("hello".into())));
    }

    // ─── serialize_project directly ───

    #[test]
    fn serialize_project_produces_valid_yaml_string() {
        let project = Project {
            name: Some("Direct Serialize".into()),
            device_settings: Vec::new(),
            chains: vec![Chain {
                id: ChainId("chain:0".into()),
                description: Some("ch1".into()),
                instrument: "generic".to_string(),
                enabled: false,
                blocks: vec![
                    AudioBlock {
                        id: BlockId("chain:0:input:0".into()),
                        enabled: true,
                        kind: AudioBlockKind::Input(InputBlock {
                            model: "standard".to_string(),
                            entries: Vec::new(),
                        }),
                    },
                    AudioBlock {
                        id: BlockId("chain:0:output:0".into()),
                        enabled: true,
                        kind: AudioBlockKind::Output(OutputBlock {
                            model: "standard".to_string(),
                            entries: Vec::new(),
                        }),
                    },
                ],
            }],
        };
        let yaml_str = super::serialize_project(&project).expect("serialize");
        assert!(yaml_str.contains("name: Direct Serialize"));
        assert!(yaml_str.contains("type: input"));
        assert!(yaml_str.contains("type: output"));
    }

    // ─── serialize_audio_blocks directly ───

    #[test]
    fn serialize_audio_blocks_returns_vec_of_values() {
        let delay_model = first_model(block_delay::supported_models());
        let blocks = vec![
            core_block("b:0", "delay", delay_model, Vec::new()),
        ];
        let values = super::serialize_audio_blocks(&blocks).expect("serialize");
        assert_eq!(values.len(), 1);
        let yaml_str = serde_yaml::to_string(&values[0]).expect("to string");
        assert!(yaml_str.contains("type: delay"));
        assert!(yaml_str.contains(&format!("model: {}", delay_model)));
    }

    // ─── ChainBlocksPreset save/load with various block types ───

    #[test]
    fn preset_roundtrips_with_core_blocks() {
        let temp_dir = tempdir().expect("temp dir");
        let path = temp_dir.path().join("multi.yaml");
        let delay_model = first_model(block_delay::supported_models());
        let reverb_model = first_model(block_reverb::supported_models());
        let preset = ChainBlocksPreset {
            id: "multi".into(),
            name: Some("Multi Block Preset".into()),
            blocks: vec![
                core_block("preset:multi:block:0", "delay", delay_model, Vec::new()),
                core_block("preset:multi:block:1", "reverb", reverb_model, Vec::new()),
            ],
        };
        save_chain_preset_file(&path, &preset).expect("save");
        let loaded = load_chain_preset_file(&path).expect("load");
        assert_eq!(loaded.id, "multi");
        assert_eq!(loaded.name, Some("Multi Block Preset".into()));
        assert_eq!(loaded.blocks.len(), 2);
        assert_eq!(loaded.blocks[0].model_ref().unwrap().model, delay_model);
        assert_eq!(loaded.blocks[1].model_ref().unwrap().model, reverb_model);
    }

    #[test]
    fn preset_roundtrips_with_no_blocks() {
        let temp_dir = tempdir().expect("temp dir");
        let path = temp_dir.path().join("empty.yaml");
        let preset = ChainBlocksPreset {
            id: "empty".into(),
            name: None,
            blocks: Vec::new(),
        };
        save_chain_preset_file(&path, &preset).expect("save");
        let loaded = load_chain_preset_file(&path).expect("load");
        assert_eq!(loaded.id, "empty");
        assert!(loaded.name.is_none());
        assert!(loaded.blocks.is_empty());
    }

    #[test]
    fn preset_roundtrips_with_input_output_blocks() {
        let temp_dir = tempdir().expect("temp dir");
        let path = temp_dir.path().join("io_preset.yaml");
        let preset = ChainBlocksPreset {
            id: "io_preset".into(),
            name: Some("IO Preset".into()),
            blocks: vec![
                AudioBlock {
                    id: BlockId("preset:io_preset:block:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".to_string(),
                        entries: vec![InputEntry {
                            device_id: DeviceId("mic-dev".into()),
                            mode: ChainInputMode::Mono,
                            channels: vec![0],
                        }],
                    }),
                },
                AudioBlock {
                    id: BlockId("preset:io_preset:block:1".into()),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".to_string(),
                        entries: vec![OutputEntry {
                            device_id: DeviceId("spk-dev".into()),
                            mode: ChainOutputMode::Stereo,
                            channels: vec![0, 1],
                        }],
                    }),
                },
            ],
        };
        save_chain_preset_file(&path, &preset).expect("save");
        let loaded = load_chain_preset_file(&path).expect("load");
        assert_eq!(loaded.blocks.len(), 2);
        assert!(matches!(&loaded.blocks[0].kind, AudioBlockKind::Input(inp) if inp.entries[0].device_id == DeviceId("mic-dev".into())));
        assert!(matches!(&loaded.blocks[1].kind, AudioBlockKind::Output(out) if out.entries[0].device_id == DeviceId("spk-dev".into())));
    }

    // ─── Error cases ───

    #[test]
    fn load_project_fails_on_invalid_yaml() {
        let temp_dir = tempdir().expect("temp dir");
        let path = temp_dir.path().join("bad.yaml");
        fs::write(&path, "{{{{not valid yaml!!!!").expect("write");
        let repo = YamlProjectRepository { path };
        let result = repo.load_current_project();
        assert!(result.is_err());
    }

    #[test]
    fn load_project_fails_on_missing_chains_field() {
        let temp_dir = tempdir().expect("temp dir");
        let path = temp_dir.path().join("no_chains.yaml");
        fs::write(&path, "name: Missing Chains\n").expect("write");
        let repo = YamlProjectRepository { path };
        let result = repo.load_current_project();
        assert!(result.is_err());
    }

    #[test]
    fn load_project_fails_on_nonexistent_file() {
        let repo = YamlProjectRepository {
            path: PathBuf::from("/tmp/does_not_exist_openrig_test.yaml"),
        };
        let result = repo.load_current_project();
        assert!(result.is_err());
    }

    #[test]
    fn load_preset_fails_on_invalid_yaml() {
        let temp_dir = tempdir().expect("temp dir");
        let path = temp_dir.path().join("bad_preset.yaml");
        fs::write(&path, ":::not yaml:::").expect("write");
        let result = load_chain_preset_file(&path);
        assert!(result.is_err());
    }

    // ─── yaml_scalar_to_parameter_value edge cases ───

    #[test]
    fn yaml_scalar_sequence_returns_error() {
        let seq = serde_yaml::Value::Sequence(vec![serde_yaml::Value::Bool(true)]);
        let result = super::yaml_scalar_to_parameter_value(seq);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unsupported yaml value"));
    }

    #[test]
    fn yaml_key_non_string_returns_error() {
        let key = serde_yaml::Value::Bool(true);
        let result = super::yaml_key_to_string(key);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("keys must be strings"));
    }

    // ─── Device settings roundtrip ───

    #[test]
    fn device_settings_not_persisted_in_yaml() {
        use project::device::DeviceSettings;
        let temp_dir = tempdir().expect("temp dir");
        let path = temp_dir.path().join("with_devices.yaml");
        let repo = YamlProjectRepository { path: path.clone() };
        let project = Project {
            name: Some("With Devices".into()),
            device_settings: vec![
                DeviceSettings {
                    device_id: DeviceId("coreaudio:builtin".into()),
                    sample_rate: 48000,
                    buffer_size_frames: 256,
                    bit_depth: 32,
                    realtime: true,
                    rt_priority: 70,
                    nperiods: 2,
                },
            ],
            chains: Vec::new(),
        };
        repo.save_project(&project).expect("save");
        // device_settings are no longer written to YAML (per-machine config)
        let yaml_content = fs::read_to_string(&path).expect("read");
        assert!(!yaml_content.contains("device_settings"));
        let loaded = repo.load_current_project().expect("load");
        assert_eq!(loaded.device_settings.len(), 0);
    }

    #[test]
    fn legacy_device_settings_still_deserialize() {
        let temp_dir = tempdir().expect("temp dir");
        let path = temp_dir.path().join("legacy.yaml");
        let delay_model = first_model(block_delay::supported_models());
        fs::write(&path, format!(
            "name: Legacy\ndevice_settings:\n  - device_id: \"coreaudio:builtin\"\n    sample_rate: 48000\n    buffer_size_frames: 256\nchains:\n  - description: ch1\n    instrument: electric_guitar\n    blocks:\n      - type: input\n        model: standard\n        enabled: true\n        entries:\n          - name: In\n            device_id: \"coreaudio:builtin\"\n            mode: mono\n            channels: [0]\n      - type: delay\n        model: {}\n        enabled: true\n        params:\n          time_ms: 300.0\n          feedback: 40.0\n          mix: 30.0\n      - type: output\n        model: standard\n        enabled: true\n        entries:\n          - name: Out\n            device_id: \"coreaudio:builtin\"\n            mode: stereo\n            channels: [0, 1]\n",
            delay_model
        )).expect("write");
        let repo = YamlProjectRepository { path };
        let loaded = repo.load_current_project().expect("load");
        // Legacy device_settings are still read for backward compat
        assert_eq!(loaded.device_settings.len(), 1);
        assert_eq!(loaded.device_settings[0].device_id, DeviceId("coreaudio:builtin".into()));
    }

    // ─── Inline I/O in blocks (new format) takes precedence ───

    #[test]
    fn load_project_inline_io_ignores_legacy_sections() {
        let temp_dir = tempdir().expect("temp dir");
        let project_path = temp_dir.path().join("inline_io.yaml");
        let delay_model = first_model(block_delay::supported_models());
        fs::write(
            &project_path,
            format!(
                r#"
chains:
  - description: Inline IO
    instrument: electric_guitar
    inputs:
      - device_id: should-be-ignored
        channels: [99]
    outputs:
      - device_id: should-be-ignored-too
        channels: [99]
    blocks:
      - type: input
        enabled: true
        model: standard
        entries:
          - name: Real Input
            device_id: real-input
            mode: mono
            channels: [0]
      - type: delay
        model: {delay_model}
        params:
          time_ms: 100
          feedback: 20
          mix: 30
      - type: output
        enabled: true
        model: standard
        entries:
          - name: Real Output
            device_id: real-output
            mode: stereo
            channels: [0, 1]
"#,
            ),
        )
        .expect("write");
        let repo = YamlProjectRepository { path: project_path };
        let project = repo.load_current_project().expect("load");
        let chain = &project.chains[0];
        // Inline IO wins: should have the real-input device, not the legacy one
        let inputs = chain.input_blocks();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].1.entries[0].device_id, DeviceId("real-input".into()));
        let outputs = chain.output_blocks();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].1.entries[0].device_id, DeviceId("real-output".into()));
    }

    // ─── Legacy single input_device_id output with mono channels ───

    #[test]
    fn load_project_legacy_single_device_mono_output() {
        let temp_dir = tempdir().expect("temp dir");
        let project_path = temp_dir.path().join("legacy_mono.yaml");
        fs::write(
            &project_path,
            r#"
chains:
  - enabled: true
    input_device_id: mono-input
    input_channels: [0]
    output_device_id: mono-output
    output_channels: [0]
    blocks: []
"#,
        )
        .expect("write");
        let repo = YamlProjectRepository { path: project_path };
        let project = repo.load_current_project().expect("load");
        let outputs = project.chains[0].output_blocks();
        assert_eq!(outputs[0].1.entries[0].mode, ChainOutputMode::Mono);
    }

    // ─── Default instrument ───

    #[test]
    fn load_project_defaults_instrument_to_electric_guitar() {
        let temp_dir = tempdir().expect("temp dir");
        let project_path = temp_dir.path().join("no_instrument.yaml");
        fs::write(
            &project_path,
            r#"
chains:
  - description: no instrument field
    blocks:
      - type: input
        enabled: true
        model: standard
        entries: []
      - type: output
        enabled: true
        model: standard
        entries: []
"#,
        )
        .expect("write");
        let repo = YamlProjectRepository { path: project_path };
        let project = repo.load_current_project().expect("load");
        assert_eq!(project.chains[0].instrument, "electric_guitar");
    }

    // ─── Nam block roundtrip ───

    #[test]
    fn roundtrip_nam_block_preserves_model_and_params() {
        use domain::value_objects::ParameterValue;
        use project::block::NamBlock;
        let nam_model = first_model(block_nam::supported_models());
        let schema = project::block::schema_for_block_model("nam", nam_model)
            .expect("nam schema");
        let mut params = ParameterSet::default();
        params.insert("model_path", ParameterValue::String("/tmp/test.nam".into()));
        let params = params.normalized_against(&schema).expect("normalize");
        let block = AudioBlock {
            id: BlockId("chain:0:block:0".into()),
            enabled: true,
            kind: AudioBlockKind::Nam(NamBlock {
                model: nam_model.to_string(),
                params,
            }),
        };
        let yaml = super::AudioBlockYaml::from_audio_block(&block).expect("to yaml");
        let value = serde_yaml::to_value(&yaml).expect("serialize");
        let parsed: super::AudioBlockYaml = serde_yaml::from_value(value).expect("deserialize");
        let chain_id = ChainId("chain:0".to_string());
        let restored = parsed.into_audio_block(&chain_id, 0).expect("into block");
        match &restored.kind {
            AudioBlockKind::Nam(nam) => assert_eq!(nam.model, nam_model),
            other => panic!("expected Nam block, got {:?}", other),
        }
    }

    // ─── Multiple chains in a project ───

    #[test]
    fn project_with_multiple_chains_roundtrips() {
        let temp_dir = tempdir().expect("temp dir");
        let path = temp_dir.path().join("multi_chain.yaml");
        let repo = YamlProjectRepository { path: path.clone() };
        let project = Project {
            name: Some("Multi Chain".into()),
            device_settings: Vec::new(),
            chains: vec![
                Chain {
                    id: ChainId("chain:0".into()),
                    description: Some("Guitar".into()),
                    instrument: "electric_guitar".to_string(),
                    enabled: false,
                    blocks: vec![
                        AudioBlock {
                            id: BlockId("chain:0:input:0".into()),
                            enabled: true,
                            kind: AudioBlockKind::Input(InputBlock {
                                model: "standard".to_string(),
                                entries: Vec::new(),
                            }),
                        },
                        AudioBlock {
                            id: BlockId("chain:0:output:0".into()),
                            enabled: true,
                            kind: AudioBlockKind::Output(OutputBlock {
                                model: "standard".to_string(),
                                entries: Vec::new(),
                            }),
                        },
                    ],
                },
                Chain {
                    id: ChainId("chain:1".into()),
                    description: Some("Bass".into()),
                    instrument: "bass".to_string(),
                    enabled: false,
                    blocks: vec![
                        AudioBlock {
                            id: BlockId("chain:1:input:0".into()),
                            enabled: true,
                            kind: AudioBlockKind::Input(InputBlock {
                                model: "standard".to_string(),
                                entries: Vec::new(),
                            }),
                        },
                        AudioBlock {
                            id: BlockId("chain:1:output:0".into()),
                            enabled: true,
                            kind: AudioBlockKind::Output(OutputBlock {
                                model: "standard".to_string(),
                                entries: Vec::new(),
                            }),
                        },
                    ],
                },
            ],
        };
        repo.save_project(&project).expect("save");
        let loaded = repo.load_current_project().expect("load");
        assert_eq!(loaded.chains.len(), 2);
        assert_eq!(loaded.chains[0].description, Some("Guitar".into()));
        assert_eq!(loaded.chains[0].instrument, "electric_guitar");
        assert_eq!(loaded.chains[1].description, Some("Bass".into()));
        assert_eq!(loaded.chains[1].instrument, "bass");
    }

    // ─── insert_yaml_value with empty path is a no-op ───

    #[test]
    fn insert_yaml_value_empty_path_is_noop() {
        let mut mapping = serde_yaml::Mapping::new();
        super::insert_yaml_value(&mut mapping, &[], serde_yaml::Value::Bool(true));
        assert!(mapping.is_empty());
    }

    // ─── Inline input block with legacy single device_id field ───

    #[test]
    fn inline_input_block_legacy_device_id_migrates() {
        let temp_dir = tempdir().expect("temp dir");
        let project_path = temp_dir.path().join("inline_legacy_input.yaml");
        fs::write(
            &project_path,
            r#"
chains:
  - description: Inline legacy
    blocks:
      - type: input
        enabled: true
        model: standard
        device_id: legacy-dev
        mode: stereo
        channels: [0, 1]
      - type: output
        enabled: true
        model: standard
        entries: []
"#,
        )
        .expect("write");
        let repo = YamlProjectRepository { path: project_path };
        let project = repo.load_current_project().expect("load");
        let inputs = project.chains[0].input_blocks();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].1.entries[0].device_id, DeviceId("legacy-dev".into()));
        assert_eq!(inputs[0].1.entries[0].channels, vec![0, 1]);
    }

    // ─── Disabled block roundtrip ───

    #[test]
    fn disabled_core_block_preserves_enabled_false() {
        let delay_model = first_model(block_delay::supported_models());
        let mut block = core_block("chain:0:block:0", "delay", delay_model, Vec::new());
        block.enabled = false;

        let yaml = super::AudioBlockYaml::from_audio_block(&block).expect("to yaml");
        let value = serde_yaml::to_value(&yaml).expect("serialize");
        let parsed: super::AudioBlockYaml = serde_yaml::from_value(value).expect("deserialize");
        let chain_id = ChainId("chain:0".to_string());
        let restored = parsed.into_audio_block(&chain_id, 0).expect("into block");
        assert!(!restored.enabled);
    }

    // ─── from_audio_block with unsupported effect_type returns error ───

    #[test]
    fn from_audio_block_unsupported_effect_type_returns_error() {
        let block = AudioBlock {
            id: BlockId("chain:0:block:0".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "nonexistent_type".to_string(),
                model: "foo".to_string(),
                params: ParameterSet::default(),
            }),
        };
        let result = super::AudioBlockYaml::from_audio_block(&block);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unsupported core block effect_type"));
    }
}
