//! YAML <-> AudioBlock conversion. The AudioBlockYaml enum mirrors
//! AudioBlockKind one-to-one and owns all the per-effect-type
//! deserialization logic. Lifted out of `lib.rs` so the production file
//! stays under the size cap.

use anyhow::{anyhow, Context, Result};
use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    normalize_block_params, AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry,
    InsertBlock, InsertEndpoint, NamBlock, OutputBlock, OutputEntry, SelectBlock,
};
use project::chain::{ChainInputMode, ChainOutputMode};
use project::param::ParameterSet;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::{
    default_amp_model, default_body_model, default_cab_model, default_delay_model,
    default_drive_model, default_dynamics_model, default_enabled, default_filter_model,
    default_full_rig_model, default_io_yaml_model, default_ir_model, default_modulation_model,
    default_nam_model, default_pitch_model, default_preamp_model, default_reverb_model,
    default_utility_model, default_wah_model, flatten_parameter_set, generated_block_id,
    parameter_set_to_yaml_value, yaml_scalar_to_parameter_value,
    ChainInputEntryYaml, ChainOutputEntryYaml,
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum AudioBlockYaml {
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
pub(crate) struct InsertEndpointYaml {
    #[serde(default)]
    device_id: String,
    #[serde(default)]
    mode: ChainInputMode,
    #[serde(default)]
    channels: Vec<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct SelectOptionYaml {
    id: String,
    #[serde(flatten)]
    block: AudioBlockYaml,
}

impl AudioBlockYaml {
    pub(crate) fn into_audio_block(self, chain_id: &ChainId, index: usize) -> Result<AudioBlock> {
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

    pub(crate) fn from_audio_block(block: &AudioBlock) -> Result<Self> {
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

pub(crate) fn load_audio_block_value(value: Value, chain_id: &ChainId, index: usize) -> Option<AudioBlock> {
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

pub(crate) fn extract_core_block_fields(yaml: AudioBlockYaml) -> (&'static str, bool, String, Value) {
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
