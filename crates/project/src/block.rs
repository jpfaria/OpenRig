use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use serde::{Deserialize, Serialize};
use block_amp_combo::{amp_combo_model_schema, validate_amp_combo_params};
use block_amp_head::{amp_head_model_schema, validate_amp_head_params};
use block_cab::{cab_model_schema, validate_cab_params};
use block_core::ModelAudioMode;
use block_delay::delay_model_schema;
use block_dyn::{compressor_supported_models, dynamics_model_schema, gate_supported_models};
use block_filter::filter_model_schema;
use block_full_rig::{full_rig_model_schema, validate_full_rig_params};
use block_gain::{drive_model_schema, validate_drive_params};
use block_ir::{ir_model_schema, validate_ir_params};
use block_mod::modulation_model_schema;
use block_nam::nam_model_schema;
use block_reverb::reverb_model_schema;
use block_util::{supported_models as utility_supported_models, utility_model_schema};
use block_wah::{validate_wah_params, wah_model_schema};

use crate::param::{BlockParameterDescriptor, ModelParameterSchema, ParameterSet};

macro_rules! define_model_block {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        pub struct $name {
            pub model: String,
            pub params: ParameterSet,
        }
    };
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioBlock {
    pub id: BlockId,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub kind: AudioBlockKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockAudioDescriptor {
    pub block_id: BlockId,
    pub effect_type: String,
    pub model: String,
    pub display_name: String,
    pub audio_mode: ModelAudioMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AudioBlockKind {
    Nam(NamBlock),
    Core(CoreBlock),
    Select(SelectBlock),
}

define_model_block!(NamBlock);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreBlock {
    pub kind: CoreBlockKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CoreBlockKind {
    AmpHead(AmpHeadBlock),
    AmpCombo(AmpComboBlock),
    FullRig(FullRigBlock),
    Cab(CabBlock),
    Ir(IrBlock),
    Drive(DriveBlock),
    Compressor(CompressorBlock),
    Gate(GateBlock),
    Eq(EqBlock),
    Wah(WahBlock),
    Tremolo(TremoloBlock),
    Delay(DelayBlock),
    Reverb(ReverbBlock),
    Tuner(TunerBlock),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectBlock {
    pub selected_block_id: BlockId,
    pub options: Vec<AudioBlock>,
}

define_model_block!(AmpHeadBlock);
define_model_block!(AmpComboBlock);
define_model_block!(FullRigBlock);
define_model_block!(CabBlock);
define_model_block!(IrBlock);
define_model_block!(DriveBlock);
define_model_block!(CompressorBlock);
define_model_block!(GateBlock);
define_model_block!(EqBlock);
define_model_block!(WahBlock);
define_model_block!(TremoloBlock);
define_model_block!(DelayBlock);
define_model_block!(ReverbBlock);
define_model_block!(TunerBlock);

#[derive(Clone, Copy)]
pub struct BlockModelRef<'a> {
    pub effect_type: &'static str,
    pub model: &'a str,
    pub params: &'a ParameterSet,
}

impl AudioBlock {
    pub fn validate_params(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        match &self.kind {
            AudioBlockKind::Nam(stage) => {
                normalize_block_params("nam", &stage.model, stage.params.clone())?;
                Ok(())
            }
            AudioBlockKind::Core(core) => core.validate_params(),
            AudioBlockKind::Select(select) => {
                for option in &select.options {
                    option.validate_params()?;
                }
                Ok(())
            }
        }
    }

    pub fn parameter_descriptors(&self) -> Result<Vec<BlockParameterDescriptor>, String> {
        match &self.kind {
            AudioBlockKind::Nam(stage) => {
                describe_block_params(&self.id, "nam", &stage.model, &stage.params)
            }
            AudioBlockKind::Core(core) => core.parameter_descriptors(&self.id),
            AudioBlockKind::Select(select) => {
                let mut descriptors = Vec::new();
                for option in &select.options {
                    descriptors.extend(option.parameter_descriptors()?);
                }
                Ok(descriptors)
            }
        }
    }

    pub fn audio_descriptors(&self) -> Result<Vec<BlockAudioDescriptor>, String> {
        if !self.enabled {
            return Ok(Vec::new());
        }
        match &self.kind {
            AudioBlockKind::Nam(stage) => {
                Ok(vec![describe_block_audio(&self.id, "nam", &stage.model)?])
            }
            AudioBlockKind::Core(core) => core.audio_descriptors(&self.id),
            AudioBlockKind::Select(select) => {
                let mut descriptors = Vec::new();
                for option in &select.options {
                    descriptors.extend(option.audio_descriptors()?);
                }
                Ok(descriptors)
            }
        }
    }

    pub fn model_ref(&self) -> Option<BlockModelRef<'_>> {
        match &self.kind {
            AudioBlockKind::Nam(stage) => Some(BlockModelRef {
                effect_type: "nam",
                model: &stage.model,
                params: &stage.params,
            }),
            AudioBlockKind::Core(core) => Some(core.kind.model_ref()),
            AudioBlockKind::Select(_) => None,
        }
    }
}

const fn default_enabled() -> bool {
    true
}

impl CoreBlock {
    fn validate_params(&self) -> Result<(), String> {
        let stage = self.kind.model_ref();
        normalize_block_params(stage.effect_type, stage.model, stage.params.clone())?;
        Ok(())
    }

    fn parameter_descriptors(
        &self,
        block_id: &BlockId,
    ) -> Result<Vec<BlockParameterDescriptor>, String> {
        let stage = self.kind.model_ref();
        describe_block_params(block_id, stage.effect_type, stage.model, stage.params)
    }

    fn audio_descriptors(&self, block_id: &BlockId) -> Result<Vec<BlockAudioDescriptor>, String> {
        let stage = self.kind.model_ref();
        Ok(vec![describe_block_audio(
            block_id,
            stage.effect_type,
            stage.model,
        )?])
    }
}

impl CoreBlockKind {
    pub fn model_ref(&self) -> BlockModelRef<'_> {
        match self {
            CoreBlockKind::AmpHead(stage) => BlockModelRef {
                effect_type: "amp_head",
                model: &stage.model,
                params: &stage.params,
            },
            CoreBlockKind::AmpCombo(stage) => BlockModelRef {
                effect_type: "amp_combo",
                model: &stage.model,
                params: &stage.params,
            },
            CoreBlockKind::FullRig(stage) => BlockModelRef {
                effect_type: "full_rig",
                model: &stage.model,
                params: &stage.params,
            },
            CoreBlockKind::Cab(stage) => BlockModelRef {
                effect_type: "cab",
                model: &stage.model,
                params: &stage.params,
            },
            CoreBlockKind::Ir(stage) => BlockModelRef {
                effect_type: "ir",
                model: &stage.model,
                params: &stage.params,
            },
            CoreBlockKind::Drive(stage) => BlockModelRef {
                effect_type: "drive",
                model: &stage.model,
                params: &stage.params,
            },
            CoreBlockKind::Delay(stage) => BlockModelRef {
                effect_type: "delay",
                model: &stage.model,
                params: &stage.params,
            },
            CoreBlockKind::Reverb(stage) => BlockModelRef {
                effect_type: "reverb",
                model: &stage.model,
                params: &stage.params,
            },
            CoreBlockKind::Tuner(stage) => BlockModelRef {
                effect_type: "utility",
                model: &stage.model,
                params: &stage.params,
            },
            CoreBlockKind::Compressor(stage) => BlockModelRef {
                effect_type: "dynamics",
                model: &stage.model,
                params: &stage.params,
            },
            CoreBlockKind::Gate(stage) => BlockModelRef {
                effect_type: "dynamics",
                model: &stage.model,
                params: &stage.params,
            },
            CoreBlockKind::Eq(stage) => BlockModelRef {
                effect_type: "filter",
                model: &stage.model,
                params: &stage.params,
            },
            CoreBlockKind::Wah(stage) => BlockModelRef {
                effect_type: "wah",
                model: &stage.model,
                params: &stage.params,
            },
            CoreBlockKind::Tremolo(stage) => BlockModelRef {
                effect_type: "modulation",
                model: &stage.model,
                params: &stage.params,
            },
        }
    }
}

pub fn normalize_block_params(
    effect_type: &str,
    model: &str,
    params: ParameterSet,
) -> Result<ParameterSet, String> {
    let schema = schema_for_block_model(effect_type, model)?;
    let normalized = params.normalized_against(&schema)?;
    match effect_type {
        "amp_head" => {
            validate_amp_head_params(model, &normalized).map_err(|error| error.to_string())?
        }
        "amp_combo" => {
            validate_amp_combo_params(model, &normalized).map_err(|error| error.to_string())?
        }
        "full_rig" => {
            validate_full_rig_params(model, &normalized).map_err(|error| error.to_string())?
        }
        "cab" => validate_cab_params(model, &normalized).map_err(|error| error.to_string())?,
        "ir" => validate_ir_params(model, &normalized).map_err(|error| error.to_string())?,
        "drive" => validate_drive_params(model, &normalized).map_err(|error| error.to_string())?,
        "wah" => validate_wah_params(model, &normalized).map_err(|error| error.to_string())?,
        _ => {}
    }
    Ok(normalized)
}

pub fn schema_for_block_model(
    effect_type: &str,
    model: &str,
) -> Result<ModelParameterSchema, String> {
    match effect_type {
        "amp_head" => amp_head_model_schema(model).map_err(|error| error.to_string()),
        "amp_combo" => amp_combo_model_schema(model).map_err(|error| error.to_string()),
        "full_rig" => full_rig_model_schema(model).map_err(|error| error.to_string()),
        "cab" => cab_model_schema(model).map_err(|error| error.to_string()),
        "ir" => ir_model_schema(model).map_err(|error| error.to_string()),
        "drive" => drive_model_schema(model).map_err(|error| error.to_string()),
        "nam" => nam_model_schema(model).map_err(|error| error.to_string()),
        "delay" => delay_model_schema(model).map_err(|error| error.to_string()),
        "reverb" => reverb_model_schema(model).map_err(|error| error.to_string()),
        "utility" => utility_model_schema(model).map_err(|error| error.to_string()),
        "dynamics" => dynamics_model_schema(model).map_err(|error| error.to_string()),
        "filter" => filter_model_schema(model).map_err(|error| error.to_string()),
        "wah" => wah_model_schema(model).map_err(|error| error.to_string()),
        "modulation" => modulation_model_schema(model).map_err(|error| error.to_string()),
        other => Err(format!("unsupported block type '{}'", other)),
    }
}

pub fn build_audio_block_kind(
    effect_type: &str,
    model: &str,
    params: ParameterSet,
) -> Result<AudioBlockKind, String> {
    let model = model.to_string();
    let kind = match effect_type {
        "amp_head" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::AmpHead(AmpHeadBlock { model, params }),
        }),
        "amp_combo" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::AmpCombo(AmpComboBlock { model, params }),
        }),
        "full_rig" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::FullRig(FullRigBlock { model, params }),
        }),
        "cab" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::Cab(CabBlock { model, params }),
        }),
        "ir" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::Ir(IrBlock { model, params }),
        }),
        "drive" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::Drive(DriveBlock { model, params }),
        }),
        "dynamics" => {
            if compressor_supported_models().contains(&model.as_str()) {
                AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Compressor(CompressorBlock { model, params }),
                })
            } else if gate_supported_models().contains(&model.as_str()) {
                AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Gate(GateBlock { model, params }),
                })
            } else {
                return Err(format!("unsupported dynamics model '{}'", model));
            }
        }
        "filter" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::Eq(EqBlock { model, params }),
        }),
        "wah" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::Wah(WahBlock { model, params }),
        }),
        "modulation" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::Tremolo(TremoloBlock { model, params }),
        }),
        "delay" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::Delay(DelayBlock { model, params }),
        }),
        "reverb" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::Reverb(ReverbBlock { model, params }),
        }),
        "utility" => {
            if utility_supported_models().contains(&model.as_str()) {
                AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Tuner(TunerBlock { model, params }),
                })
            } else {
                return Err(format!("unsupported utility model '{}'", model));
            }
        }
        "nam" => AudioBlockKind::Nam(NamBlock { model, params }),
        other => return Err(format!("unsupported block type '{}'", other)),
    };
    Ok(kind)
}

fn describe_block_params(
    block_id: &BlockId,
    effect_type: &str,
    model: &str,
    params: &ParameterSet,
) -> Result<Vec<BlockParameterDescriptor>, String> {
    let schema = schema_for_block_model(effect_type, model)?;
    let normalized = params.normalized_against(&schema)?;
    Ok(schema
        .parameters
        .iter()
        .map(|spec| {
            let current_value = normalized
                .get(&spec.path)
                .cloned()
                .or_else(|| spec.default_value.clone())
                .unwrap_or(ParameterValue::Null);
            spec.materialize(
                block_id,
                effect_type,
                model,
                schema.audio_mode,
                current_value,
            )
        })
        .collect())
}

fn describe_block_audio(
    block_id: &BlockId,
    effect_type: &str,
    model: &str,
) -> Result<BlockAudioDescriptor, String> {
    let schema = schema_for_block_model(effect_type, model)?;
    Ok(BlockAudioDescriptor {
        block_id: block_id.clone(),
        effect_type: effect_type.to_string(),
        model: schema.model,
        display_name: schema.display_name,
        audio_mode: schema.audio_mode,
    })
}

#[cfg(test)]
mod tests {
    use super::{normalize_block_params, schema_for_block_model};
    use crate::param::ParameterSet;

    #[test]
    fn project_contract_exposes_family_schemas() {
        let families = [
            ("amp_head", block_amp_head::supported_models()),
            ("amp_combo", block_amp_combo::supported_models()),
            ("cab", block_cab::supported_models()),
            ("ir", block_ir::supported_models()),
            ("wah", block_wah::supported_models()),
            ("delay", block_delay::supported_models()),
        ];

        for (effect_type, models) in families {
            for model in models {
                let schema =
                    schema_for_block_model(effect_type, model).expect("schema should exist");
                assert_eq!(schema.model, *model);
                assert!(!schema.parameters.is_empty(), "schema for {effect_type}:{model} should expose parameters");
            }
        }
    }

    #[test]
    fn project_contract_normalizes_defaults_for_supported_families() {
        let families = [
            ("amp_head", block_amp_head::supported_models()),
            ("amp_combo", block_amp_combo::supported_models()),
            ("cab", block_cab::supported_models()),
            ("ir", block_ir::supported_models()),
            ("wah", block_wah::supported_models()),
            ("delay", block_delay::supported_models()),
        ];

        for (effect_type, models) in families {
            for model in models {
                let schema =
                    schema_for_block_model(effect_type, model).expect("schema should exist");
                let normalized = normalize_block_params(effect_type, model, ParameterSet::default());
                let has_complete_defaults = schema
                    .parameters
                    .iter()
                    .all(|parameter| parameter.default_value.is_some());

                if has_complete_defaults {
                    let normalized = normalized.expect("params should normalize with schema defaults");
                    assert_eq!(normalized.values.len(), schema.parameters.len());
                } else {
                    assert!(
                        normalized.is_err(),
                        "model {effect_type}:{model} should reject empty params when schema has required fields without defaults"
                    );
                }
            }
        }
    }
}
