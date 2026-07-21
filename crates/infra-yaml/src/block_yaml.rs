//! YAML <-> AudioBlock conversion. The AudioBlockYaml enum mirrors
//! AudioBlockKind one-to-one and owns all the per-effect-type
//! deserialization logic. Lifted out of `lib.rs` so the production file
//! stays under the size cap.

use anyhow::{anyhow, Context, Result};
use domain::ids::{BlockId, ChainId};
use project::block::{
    AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InsertBlock, NamBlock, OutputBlock,
    SelectBlock,
};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::chain_yaml::default_io_yaml_model;
use crate::{
    default_amp_model, default_body_model, default_cab_model, default_delay_model,
    default_drive_model, default_dynamics_model, default_enabled, default_filter_model,
    default_full_rig_model, default_ir_model, default_modulation_model, default_nam_model,
    default_pitch_model, default_preamp_model, default_reverb_model, default_utility_model,
    default_wah_model, generated_block_id, parameter_set_to_yaml_value,
};

// #792: the load/parse helpers moved to block_yaml_load.rs; the impl below calls them.
use crate::block_yaml_load::{
    extract_core_block_fields, load_model_params, migrate_legacy_model_id,
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
        /// Registry binding id this block reads from (model A, #716).
        #[serde(default)]
        io: String,
        /// Endpoint name within the referenced binding (model A, #716).
        #[serde(default)]
        endpoint: String,
    },
    #[serde(rename = "output")]
    Output {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_io_yaml_model")]
        model: String,
        /// Registry binding id this block writes to (model A, #716).
        #[serde(default)]
        io: String,
        /// Endpoint name within the referenced binding (model A, #716).
        #[serde(default)]
        endpoint: String,
    },
    #[serde(rename = "insert")]
    Insert {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_io_yaml_model")]
        model: String,
        /// Registry binding id for the external send/return loop (model A, #716):
        /// the send goes to this binding's output, the return comes from its input.
        #[serde(default)]
        io: String,
    },
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
                io,
                endpoint,
            } => Ok(AudioBlock {
                id: generated_id,
                enabled,
                kind: AudioBlockKind::Input(InputBlock {
                    model,
                    io,
                    endpoint,
                }),
            }),
            AudioBlockYaml::Output {
                enabled,
                model,
                io,
                endpoint,
            } => Ok(AudioBlock {
                id: generated_id,
                enabled,
                kind: AudioBlockKind::Output(OutputBlock {
                    model,
                    io,
                    endpoint,
                }),
            }),
            AudioBlockYaml::Insert { enabled, model, io } => Ok(AudioBlock {
                id: generated_id,
                enabled,
                kind: AudioBlockKind::Insert(InsertBlock { model, io }),
            }),
            other => {
                let (effect_type, enabled, model, params) = extract_core_block_fields(other);
                let model = migrate_legacy_model_id(effect_type, model, &params);

                // NAM captures are persisted under their NATURAL block type
                // ("gain" for stompbox NAMs, "amp" / "preamp" for amp NAMs) —
                // that's how the MCP `add_block` path saves them and how the
                // live chain keeps them: a `Core { effect_type: "gain" }`,
                // NOT a generic `Nam` block. Prefer that declared category so
                // a preset loaded onto another chain keeps each block's
                // widget and signal-chain role (otherwise GAIN/AMP/PREAMP all
                // collapse to purple "NAM" blocks). The declared type resolves
                // via the disk-package schema fallback whenever the plugin
                // catalog is loaded — i.e. the GUI / live path.
                match load_model_params(effect_type, &model, params.clone()) {
                    Ok(resolved) => Ok(AudioBlock {
                        id: generated_id,
                        enabled,
                        kind: AudioBlockKind::Core(CoreBlock {
                            effect_type: effect_type.to_string(),
                            model,
                            params: resolved,
                        }),
                    }),
                    // Issue #552: an offline render with no plugin catalog
                    // can't resolve the declared type. For NAM-prefixed models
                    // fall back to the generic Nam runtime so the block still
                    // survives instead of being dropped silently.
                    Err(core_err) => {
                        if model.starts_with("nam_") {
                            Ok(AudioBlock {
                                id: generated_id,
                                enabled,
                                kind: AudioBlockKind::Nam(NamBlock {
                                    model: model.clone(),
                                    params: load_model_params(
                                        block_core::EFFECT_TYPE_NAM,
                                        &model,
                                        params,
                                    )?,
                                }),
                            })
                        } else {
                            Err(core_err)
                        }
                    }
                }
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
                    block_core::EFFECT_TYPE_PREAMP => Ok(Self::Preamp {
                        enabled,
                        model,
                        params,
                    }),
                    block_core::EFFECT_TYPE_AMP => Ok(Self::Amp {
                        enabled,
                        model,
                        params,
                    }),
                    block_core::EFFECT_TYPE_FULL_RIG => Ok(Self::FullRig {
                        enabled,
                        model,
                        params,
                    }),
                    block_core::EFFECT_TYPE_CAB => Ok(Self::Cab {
                        enabled,
                        model,
                        params,
                    }),
                    block_core::EFFECT_TYPE_BODY => Ok(Self::Body {
                        enabled,
                        model,
                        params,
                    }),
                    block_core::EFFECT_TYPE_IR => Ok(Self::Ir {
                        enabled,
                        model,
                        params,
                    }),
                    block_core::EFFECT_TYPE_GAIN => Ok(Self::Gain {
                        enabled,
                        model,
                        params,
                    }),
                    block_core::EFFECT_TYPE_DELAY => Ok(Self::Delay {
                        enabled,
                        model,
                        params,
                    }),
                    block_core::EFFECT_TYPE_REVERB => Ok(Self::Reverb {
                        enabled,
                        model,
                        params,
                    }),
                    block_core::EFFECT_TYPE_UTILITY => Ok(Self::Utility {
                        enabled,
                        model,
                        params,
                    }),
                    block_core::EFFECT_TYPE_DYNAMICS => Ok(Self::Dynamics {
                        enabled,
                        model,
                        params,
                    }),
                    block_core::EFFECT_TYPE_FILTER => Ok(Self::Filter {
                        enabled,
                        model,
                        params,
                    }),
                    block_core::EFFECT_TYPE_WAH => Ok(Self::Wah {
                        enabled,
                        model,
                        params,
                    }),
                    block_core::EFFECT_TYPE_MODULATION => Ok(Self::Modulation {
                        enabled,
                        model,
                        params,
                    }),
                    block_core::EFFECT_TYPE_PITCH => Ok(Self::Pitch {
                        enabled,
                        model,
                        params,
                    }),
                    block_core::EFFECT_TYPE_VST3 => Ok(Self::Vst3 {
                        enabled,
                        model,
                        params,
                    }),
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
                            block: AudioBlockYaml::from_audio_block(option).with_context(|| {
                                format!(
                                    "failed to serialize select option {} for block '{}'",
                                    index, block.id.0
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
                io: input.io.clone(),
                endpoint: input.endpoint.clone(),
            }),
            AudioBlockKind::Output(output) => Ok(Self::Output {
                enabled: block.enabled,
                model: output.model.clone(),
                io: output.io.clone(),
                endpoint: output.endpoint.clone(),
            }),
            AudioBlockKind::Insert(insert) => Ok(Self::Insert {
                enabled: block.enabled,
                model: insert.model.clone(),
                io: insert.io.clone(),
            }),
        }
    }
}
