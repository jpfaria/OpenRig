use anyhow::{anyhow, Context, Result};
use domain::ids::{BlockId, DeviceId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{
    normalize_block_params, AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, NamBlock, OutputBlock, OutputEntry, SelectBlock,
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
    #[serde(default)]
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
            device_settings: project
                .device_settings
                .iter()
                .map(DeviceSettingsYaml::from_settings)
                .collect(),
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

#[derive(Debug, Deserialize, Serialize)]
struct DeviceSettingsYaml {
    device_id: String,
    sample_rate: u32,
    buffer_size_frames: u32,
}

impl From<DeviceSettingsYaml> for DeviceSettings {
    fn from(value: DeviceSettingsYaml) -> Self {
        Self {
            device_id: DeviceId(value.device_id),
            sample_rate: value.sample_rate,
            buffer_size_frames: value.buffer_size_frames,
        }
    }
}

impl DeviceSettingsYaml {
    fn from_settings(settings: &DeviceSettings) -> Self {
        Self {
            device_id: settings.device_id.0.clone(),
            sample_rate: settings.sample_rate,
            buffer_size_frames: settings.buffer_size_frames,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct ChainInputEntryYaml {
    device_id: String,
    #[serde(default)]
    mode: ChainInputMode,
    channels: Vec<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ChainInputYaml {
    #[serde(default = "default_input_yaml_name")]
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

fn default_input_yaml_name() -> String {
    "Input".to_string()
}

#[derive(Debug, Deserialize, Serialize)]
struct ChainOutputEntryYaml {
    device_id: String,
    #[serde(default)]
    mode: ChainOutputMode,
    channels: Vec<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ChainOutputYaml {
    #[serde(default = "default_output_yaml_name")]
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

fn default_output_yaml_name() -> String {
    "Output".to_string()
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
struct ChainYaml {
    #[serde(default)]
    description: Option<String>,
    #[serde(default = "default_instrument")]
    instrument: String,
    #[serde(default = "default_enabled", skip_serializing)]
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
        log::debug!("deserializing chain index={}, description={:?}, instrument='{}'", index, self.description, self.instrument);

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
                enabled: false,
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
                    name: inp.name,
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
                    name: out.name,
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
                        name: "Input 1".to_string(),
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
                        name: "Output 1".to_string(),
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
            enabled: false,
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
            enabled: chain.enabled,
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
        #[serde(default)]
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
        #[serde(default)]
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
                name,
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
                        name: if name.is_empty() { "Input".to_string() } else { name },
                        entries,
                    }),
                })
            }
            AudioBlockYaml::Output {
                enabled,
                name,
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
                        name: if name.is_empty() { "Output".to_string() } else { name },
                        entries,
                    }),
                })
            }
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
                name: input.name.clone(),
                entries: input.entries.iter().map(|e| ChainInputEntryYaml {
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
                name: output.name.clone(),
                entries: output.entries.iter().map(|e| ChainOutputEntryYaml {
                    device_id: e.device_id.0.clone(),
                    mode: e.mode,
                    channels: e.channels.clone(),
                }).collect(),
                device_id: None,
                mode: None,
                channels: None,
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
        AudioBlockYaml::Nam { enabled, model, params } => (block_core::EFFECT_TYPE_NAM, enabled, model, params),
        AudioBlockYaml::Select { .. } => unreachable!("Select handled before extract_core_block_fields"),
        AudioBlockYaml::Input { .. } => unreachable!("Input handled before extract_core_block_fields"),
        AudioBlockYaml::Output { .. } => unreachable!("Output handled before extract_core_block_fields"),
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
                            name: "Input 1".to_string(),
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
                            name: "Output 1".to_string(),
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
}
