use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use serde::{Deserialize, Serialize};
use block_amp::{amp_model_schema, validate_amp_params};
use block_preamp::{preamp_model_schema, validate_preamp_params};
use block_cab::{cab_model_schema, validate_cab_params};
use block_core::ModelAudioMode;
use block_delay::delay_model_schema;
use block_dyn::dynamics_model_schema;
use block_filter::filter_model_schema;
use block_full_rig::{full_rig_model_schema, validate_full_rig_params};
use block_gain::{gain_model_schema, validate_gain_params};
use block_ir::{ir_model_schema, validate_ir_params};
use block_mod::modulation_model_schema;
use block_nam::nam_model_schema;
use block_pitch::{pitch_model_schema, validate_pitch_params};
use block_reverb::reverb_model_schema;
use block_util::utility_model_schema;
use block_wah::{validate_wah_params, wah_model_schema};

use crate::param::{BlockParameterDescriptor, ModelParameterSchema, ParameterSet};

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamBlock {
    pub model: String,
    pub params: ParameterSet,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreBlock {
    pub effect_type: String,
    pub model: String,
    pub params: ParameterSet,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectBlock {
    pub selected_block_id: BlockId,
    pub options: Vec<AudioBlock>,
}

const MAX_SELECT_OPTIONS: usize = 8;

#[derive(Clone, Copy)]
pub struct BlockModelRef<'a> {
    pub effect_type: &'a str,
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
                select.validate_structure()?;
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
            AudioBlockKind::Select(select) => select
                .selected_option()
                .ok_or_else(|| "select block selected option does not exist".to_string())?
                .parameter_descriptors(),
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
            AudioBlockKind::Select(select) => select
                .selected_option()
                .ok_or_else(|| "select block selected option does not exist".to_string())?
                .audio_descriptors(),
        }
    }

    pub fn model_ref(&self) -> Option<BlockModelRef<'_>> {
        match &self.kind {
            AudioBlockKind::Nam(stage) => Some(BlockModelRef {
                effect_type: "nam",
                model: &stage.model,
                params: &stage.params,
            }),
            AudioBlockKind::Core(core) => Some(core.model_ref()),
            AudioBlockKind::Select(_) => None,
        }
    }
}

const fn default_enabled() -> bool {
    true
}

impl CoreBlock {
    pub fn model_ref(&self) -> BlockModelRef<'_> {
        BlockModelRef {
            effect_type: &self.effect_type,
            model: &self.model,
            params: &self.params,
        }
    }

    fn validate_params(&self) -> Result<(), String> {
        normalize_block_params(&self.effect_type, &self.model, self.params.clone())?;
        Ok(())
    }

    fn parameter_descriptors(
        &self,
        block_id: &BlockId,
    ) -> Result<Vec<BlockParameterDescriptor>, String> {
        describe_block_params(block_id, &self.effect_type, &self.model, &self.params)
    }

    fn audio_descriptors(&self, block_id: &BlockId) -> Result<Vec<BlockAudioDescriptor>, String> {
        Ok(vec![describe_block_audio(
            block_id,
            &self.effect_type,
            &self.model,
        )?])
    }
}

impl SelectBlock {
    pub fn selected_option(&self) -> Option<&AudioBlock> {
        self.options
            .iter()
            .find(|option| option.id == self.selected_block_id)
    }

    pub fn validate_structure(&self) -> Result<(), String> {
        if self.options.is_empty() {
            return Err("select block must define at least one option".to_string());
        }
        if self.options.len() > MAX_SELECT_OPTIONS {
            return Err(format!(
                "select block may define up to {} options",
                MAX_SELECT_OPTIONS
            ));
        }

        let mut effect_type = None::<&str>;
        for option in &self.options {
            if matches!(option.kind, AudioBlockKind::Select(_)) {
                return Err("select block options cannot themselves be select blocks".to_string());
            }

            let model = option.model_ref().ok_or_else(|| {
                format!(
                    "select block option '{}' does not expose a concrete model",
                    option.id.0
                )
            })?;

            match effect_type {
                Some(existing) if existing != model.effect_type => {
                    return Err("select block options must use the same effect type".to_string());
                }
                None => effect_type = Some(model.effect_type),
                _ => {}
            }
        }

        if self.selected_option().is_none() {
            return Err("select block selected option does not exist".to_string());
        }

        Ok(())
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
        "preamp" => {
            validate_preamp_params(model, &normalized).map_err(|error| error.to_string())?
        }
        "amp" => {
            validate_amp_params(model, &normalized).map_err(|error| error.to_string())?
        }
        "full_rig" => {
            validate_full_rig_params(model, &normalized).map_err(|error| error.to_string())?
        }
        "cab" => validate_cab_params(model, &normalized).map_err(|error| error.to_string())?,
        "ir" => validate_ir_params(model, &normalized).map_err(|error| error.to_string())?,
        "gain" => validate_gain_params(model, &normalized).map_err(|error| error.to_string())?,
        "wah" => validate_wah_params(model, &normalized).map_err(|error| error.to_string())?,
        "pitch" => validate_pitch_params(model, &normalized).map_err(|error| error.to_string())?,
        _ => {}
    }
    Ok(normalized)
}

pub fn schema_for_block_model(
    effect_type: &str,
    model: &str,
) -> Result<ModelParameterSchema, String> {
    match effect_type {
        "preamp" => preamp_model_schema(model).map_err(|error| error.to_string()),
        "amp" => amp_model_schema(model).map_err(|error| error.to_string()),
        "full_rig" => full_rig_model_schema(model).map_err(|error| error.to_string()),
        "cab" => cab_model_schema(model).map_err(|error| error.to_string()),
        "ir" => ir_model_schema(model).map_err(|error| error.to_string()),
        "gain" => gain_model_schema(model).map_err(|error| error.to_string()),
        "nam" => nam_model_schema(model).map_err(|error| error.to_string()),
        "delay" => delay_model_schema(model).map_err(|error| error.to_string()),
        "reverb" => reverb_model_schema(model).map_err(|error| error.to_string()),
        "utility" => utility_model_schema(model).map_err(|error| error.to_string()),
        "dynamics" => dynamics_model_schema(model).map_err(|error| error.to_string()),
        "filter" => filter_model_schema(model).map_err(|error| error.to_string()),
        "wah" => wah_model_schema(model).map_err(|error| error.to_string()),
        "pitch" => pitch_model_schema(model).map_err(|error| error.to_string()),
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
        "preamp" | "amp" | "full_rig" | "cab" | "ir" | "gain" | "dynamics" | "filter" | "wah"
        | "pitch" | "modulation" | "delay" | "reverb" | "utility" => {
            AudioBlockKind::Core(CoreBlock {
                effect_type: effect_type.to_string(),
                model,
                params,
            })
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
    use super::{
        normalize_block_params, schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock,
        SelectBlock,
    };
    use crate::param::ParameterSet;
    use domain::ids::BlockId;

    #[test]
    fn project_contract_exposes_family_schemas() {
        let families = [
            ("preamp", block_preamp::supported_models()),
            ("amp", block_amp::supported_models()),
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
                assert_eq!(
                    schema.effect_type, effect_type,
                    "schema for {effect_type}:{model} should expose matching family"
                );
                assert!(!schema.parameters.is_empty(), "schema for {effect_type}:{model} should expose parameters");
            }
        }
    }

    #[test]
    fn project_contract_normalizes_defaults_for_supported_families() {
        let families = [
            ("preamp", block_preamp::supported_models()),
            ("amp", block_amp::supported_models()),
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

    #[test]
    fn select_block_requires_at_least_one_option() {
        let block = AudioBlock {
            id: BlockId("chain:0:block:0".into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId("chain:0:block:0::missing".into()),
                options: Vec::new(),
            }),
        };

        let error = block
            .validate_params()
            .expect_err("empty select options should fail");

        assert!(error.contains("at least one option"));
    }

    #[test]
    fn select_block_rejects_missing_selected_option() {
        let first_model = block_delay::supported_models()
            .first()
            .expect("block-delay must expose at least one model");

        let block = AudioBlock {
            id: BlockId("chain:0:block:0".into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId("chain:0:block:0::missing".into()),
                options: vec![delay_block("chain:0:block:0::a", first_model)],
            }),
        };

        let error = block
            .validate_params()
            .expect_err("select without selected option should fail");

        assert!(error.contains("selected option"));
    }

    #[test]
    fn select_block_rejects_mixed_effect_types() {
        let delay_model = block_delay::supported_models()
            .first()
            .expect("block-delay must expose at least one model");
        let reverb_model = block_reverb::supported_models()
            .first()
            .expect("block-reverb must expose at least one model");

        let block = AudioBlock {
            id: BlockId("chain:0:block:0".into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId("chain:0:block:0::delay".into()),
                options: vec![
                    delay_block("chain:0:block:0::delay", delay_model),
                    reverb_block("chain:0:block:0::reverb", reverb_model),
                ],
            }),
        };

        let error = block
            .validate_params()
            .expect_err("mixed select families should fail");

        assert!(error.contains("same effect type"));
    }

    #[test]
    fn select_block_rejects_more_than_eight_options() {
        let model = block_delay::supported_models()
            .first()
            .expect("block-delay must expose at least one model");
        let options = (0..9)
            .map(|index| delay_block(format!("chain:0:block:0::{index}"), model))
            .collect::<Vec<_>>();

        let block = AudioBlock {
            id: BlockId("chain:0:block:0".into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId("chain:0:block:0::0".into()),
                options,
            }),
        };

        let error = block
            .validate_params()
            .expect_err("select with more than eight options should fail");

        assert!(error.contains("up to 8 options"));
    }

    fn delay_block(id: impl Into<String>, model: &str) -> AudioBlock {
        let schema = schema_for_block_model("delay", model).expect("delay schema");
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("delay defaults should normalize");
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

    fn reverb_block(id: impl Into<String>, model: &str) -> AudioBlock {
        let schema = schema_for_block_model("reverb", model).expect("reverb schema");
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("reverb defaults should normalize");
        AudioBlock {
            id: BlockId(id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "reverb".to_string(),
                model: model.to_string(),
                params,
            }),
        }
    }
}
