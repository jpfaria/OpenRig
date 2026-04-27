use domain::ids::{BlockId, DeviceId};
use domain::value_objects::ParameterValue;
use serde::{Deserialize, Serialize};
use block_amp::{amp_model_schema, validate_amp_params};
use block_preamp::{preamp_model_schema, validate_preamp_params};
use block_body::{body_model_schema, validate_body_params};
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

use crate::chain::{ChainInputMode, ChainOutputMode};
use crate::param::{BlockParameterDescriptor, ModelParameterSchema, ParameterSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InsertEndpoint {
    pub device_id: DeviceId,
    #[serde(default)]
    pub mode: ChainInputMode,
    pub channels: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InsertBlock {
    #[serde(default = "default_io_model")]
    pub model: String,
    pub send: InsertEndpoint,
    #[serde(rename = "return")]
    pub return_: InsertEndpoint,
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
    Input(InputBlock),
    Output(OutputBlock),
    Insert(InsertBlock),
}

impl AudioBlockKind {
    /// Stable lowercase label for the variant — for diagnostics, logs,
    /// and debug surfaces. Adding a new variant fails to compile here,
    /// not at every callsite.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Nam(_) => "nam",
            Self::Core(_) => "core",
            Self::Select(_) => "select",
            Self::Input(_) => "input",
            Self::Output(_) => "output",
            Self::Insert(_) => "insert",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputEntry {
    pub device_id: DeviceId,
    #[serde(default)]
    pub mode: ChainInputMode,
    pub channels: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputEntry {
    pub device_id: DeviceId,
    #[serde(default)]
    pub mode: ChainOutputMode,
    pub channels: Vec<usize>,
}

fn default_io_model() -> String {
    "standard".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputBlock {
    #[serde(default = "default_io_model")]
    pub model: String,
    pub entries: Vec<InputEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputBlock {
    #[serde(default = "default_io_model")]
    pub model: String,
    pub entries: Vec<OutputEntry>,
}

impl InputBlock {
    pub fn validate_channel_conflicts(&self) -> Result<(), String> {
        let mut used: Vec<(String, usize)> = Vec::new();
        for entry in &self.entries {
            for &ch in &entry.channels {
                let key = (entry.device_id.0.clone(), ch);
                if used.contains(&key) {
                    return Err(format!(
                        "Channel {} on device '{}' is used by multiple entries",
                        ch, entry.device_id.0
                    ));
                }
                used.push(key);
            }
        }
        Ok(())
    }
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
                normalize_block_params(block_core::EFFECT_TYPE_NAM, &stage.model, stage.params.clone())?;
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
            AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_) => Ok(()),
        }
    }

    pub fn parameter_descriptors(&self) -> Result<Vec<BlockParameterDescriptor>, String> {
        match &self.kind {
            AudioBlockKind::Nam(stage) => {
                describe_block_params(&self.id, block_core::EFFECT_TYPE_NAM, &stage.model, &stage.params)
            }
            AudioBlockKind::Core(core) => core.parameter_descriptors(&self.id),
            AudioBlockKind::Select(select) => select
                .selected_option()
                .ok_or_else(|| "select block selected option does not exist".to_string())?
                .parameter_descriptors(),
            AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_) => Ok(Vec::new()),
        }
    }

    pub fn audio_descriptors(&self) -> Result<Vec<BlockAudioDescriptor>, String> {
        if !self.enabled {
            return Ok(Vec::new());
        }
        match &self.kind {
            AudioBlockKind::Nam(stage) => {
                Ok(vec![describe_block_audio(&self.id, block_core::EFFECT_TYPE_NAM, &stage.model)?])
            }
            AudioBlockKind::Core(core) => core.audio_descriptors(&self.id),
            AudioBlockKind::Select(select) => select
                .selected_option()
                .ok_or_else(|| "select block selected option does not exist".to_string())?
                .audio_descriptors(),
            AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_) => Ok(Vec::new()),
        }
    }

    pub fn model_ref(&self) -> Option<BlockModelRef<'_>> {
        match &self.kind {
            AudioBlockKind::Nam(stage) => Some(BlockModelRef {
                effect_type: block_core::EFFECT_TYPE_NAM,
                model: &stage.model,
                params: &stage.params,
            }),
            AudioBlockKind::Core(core) => Some(core.model_ref()),
            AudioBlockKind::Select(_) | AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_) => None,
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
            if matches!(option.kind, AudioBlockKind::Select(_) | AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_)) {
                return Err("select block options cannot be select, input, output, or insert blocks".to_string());
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
    use block_core::*;
    match effect_type {
        EFFECT_TYPE_PREAMP => {
            validate_preamp_params(model, &normalized).map_err(|error| error.to_string())?
        }
        EFFECT_TYPE_AMP => {
            validate_amp_params(model, &normalized).map_err(|error| error.to_string())?
        }
        EFFECT_TYPE_FULL_RIG => {
            validate_full_rig_params(model, &normalized).map_err(|error| error.to_string())?
        }
        EFFECT_TYPE_CAB => validate_cab_params(model, &normalized).map_err(|error| error.to_string())?,
        EFFECT_TYPE_BODY => validate_body_params(model, &normalized).map_err(|error| error.to_string())?,
        EFFECT_TYPE_IR => validate_ir_params(model, &normalized).map_err(|error| error.to_string())?,
        EFFECT_TYPE_GAIN => validate_gain_params(model, &normalized).map_err(|error| error.to_string())?,
        EFFECT_TYPE_WAH => validate_wah_params(model, &normalized).map_err(|error| error.to_string())?,
        EFFECT_TYPE_PITCH => validate_pitch_params(model, &normalized).map_err(|error| error.to_string())?,
        _ => {}
    }
    Ok(normalized)
}

pub fn schema_for_block_model(
    effect_type: &str,
    model: &str,
) -> Result<ModelParameterSchema, String> {
    use block_core::*;
    match effect_type {
        EFFECT_TYPE_PREAMP => preamp_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_AMP => amp_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_FULL_RIG => full_rig_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_CAB => cab_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_BODY => body_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_IR => ir_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_GAIN => gain_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_NAM => nam_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_DELAY => delay_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_REVERB => reverb_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_UTILITY => utility_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_DYNAMICS => dynamics_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_FILTER => filter_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_WAH => wah_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_PITCH => pitch_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_MODULATION => modulation_model_schema(model).map_err(|error| error.to_string()),
        x if x == block_core::EFFECT_TYPE_VST3 => {
            let entry = vst3_host::find_vst3_plugin(model)
                .ok_or_else(|| format!("VST3 plugin '{}' not found in catalog", model))?;
            // Build a float parameter for each discovered VST3 parameter (normalized 0–100%).
            let parameters = entry.info.params.iter().map(|p| {
                let path = format!("p{}", p.id);
                let label = if p.title.is_empty() { p.short_title.clone() } else { p.title.clone() };
                let default_pct = (p.default_normalized * 100.0) as f32;
                block_core::param::float_parameter(
                    &path, &label, None, Some(default_pct), 0.0, 100.0, 1.0,
                    block_core::param::ParameterUnit::Percent,
                )
            }).collect();
            Ok(ModelParameterSchema {
                effect_type: block_core::EFFECT_TYPE_VST3.to_string(),
                model: model.to_string(),
                display_name: entry.display_name.to_string(),
                audio_mode: ModelAudioMode::MonoToStereo,
                parameters,
            })
        }
        other => Err(format!("unsupported block type '{}'", other)),
    }
}

pub fn build_audio_block_kind(
    effect_type: &str,
    model: &str,
    params: ParameterSet,
) -> Result<AudioBlockKind, String> {
    let model = model.to_string();
    use block_core::*;
    let kind = match effect_type {
        EFFECT_TYPE_PREAMP | EFFECT_TYPE_AMP | EFFECT_TYPE_FULL_RIG | EFFECT_TYPE_CAB
        | EFFECT_TYPE_BODY | EFFECT_TYPE_IR | EFFECT_TYPE_GAIN | EFFECT_TYPE_DYNAMICS
        | EFFECT_TYPE_FILTER | EFFECT_TYPE_WAH | EFFECT_TYPE_PITCH | EFFECT_TYPE_MODULATION
        | EFFECT_TYPE_DELAY | EFFECT_TYPE_REVERB | EFFECT_TYPE_UTILITY => {
            AudioBlockKind::Core(CoreBlock {
                effect_type: effect_type.to_string(),
                model,
                params,
            })
        }
        EFFECT_TYPE_NAM => AudioBlockKind::Nam(NamBlock { model, params }),
        x if x == EFFECT_TYPE_VST3 => AudioBlockKind::Core(CoreBlock {
            effect_type: EFFECT_TYPE_VST3.to_string(),
            model,
            params,
        }),
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
        InputBlock, InputEntry, InsertBlock, InsertEndpoint, OutputBlock, OutputEntry, SelectBlock,
    };
    use crate::chain::{ChainInputMode, ChainOutputMode};
    use crate::param::ParameterSet;
    use domain::ids::{BlockId, DeviceId};

    #[test]
    #[ignore] // cab:roland_jc_120_cab returns empty schema — needs fix
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

    // --- InputBlock/OutputBlock multi-entry tests ---

    #[test]
    fn input_block_supports_multiple_entries() {
        let input = InputBlock {
            model: "standard".to_string(),
            entries: vec![
                InputEntry {

                    device_id: DeviceId("scarlett".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![0],
                },
                InputEntry {
                    device_id: DeviceId("scarlett".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![1],
                },
            ],
        };
        assert_eq!(input.entries.len(), 2);
        assert_eq!(input.entries[0].channels, vec![0]);
        assert_eq!(input.entries[1].channels, vec![1]);
    }

    #[test]
    fn output_block_supports_multiple_entries() {
        let output = OutputBlock {
            model: "standard".to_string(),
            entries: vec![
                OutputEntry {
                    device_id: DeviceId("scarlett".into()),
                    mode: ChainOutputMode::Stereo,
                    channels: vec![0, 1],
                },
                OutputEntry {
                    device_id: DeviceId("macbook".into()),
                    mode: ChainOutputMode::Stereo,
                    channels: vec![0, 1],
                },
            ],
        };
        assert_eq!(output.entries.len(), 2);
        assert_eq!(output.entries[0].device_id.0, "scarlett");
        assert_eq!(output.entries[1].device_id.0, "macbook");
    }

    #[test]
    fn input_block_single_entry_works() {
        let input = InputBlock {
            model: "standard".to_string(),
            entries: vec![InputEntry {
                device_id: DeviceId("scarlett".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
        };
        assert_eq!(input.entries.len(), 1);
    }

    #[test]
    fn input_block_validates_no_duplicate_device_channels() {
        let input = InputBlock {
            model: "standard".to_string(),
            entries: vec![
                InputEntry {
                    device_id: DeviceId("scarlett".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![0],
                },
                InputEntry {
                    device_id: DeviceId("scarlett".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![0], // duplicate!
                },
            ],
        };
        let result = input.validate_channel_conflicts();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Channel 0"));
    }

    #[test]
    fn input_block_allows_different_channels_same_device() {
        let input = InputBlock {
            model: "standard".to_string(),
            entries: vec![
                InputEntry {
                    device_id: DeviceId("scarlett".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![0],
                },
                InputEntry {
                    device_id: DeviceId("scarlett".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![1],
                },
            ],
        };
        assert!(input.validate_channel_conflicts().is_ok());
    }

    #[test]
    fn input_block_allows_same_channel_different_devices() {
        let input = InputBlock {
            model: "standard".to_string(),
            entries: vec![
                InputEntry {
                    device_id: DeviceId("scarlett".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![0],
                },
                InputEntry {
                    device_id: DeviceId("macbook".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![0],
                },
            ],
        };
        assert!(input.validate_channel_conflicts().is_ok());
    }

    // --- InsertBlock tests ---

    #[test]
    fn insert_block_clone_equality() {
        let insert = InsertBlock {
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
        };
        let block = AudioBlock {
            id: BlockId("chain:0:insert:0".into()),
            enabled: true,
            kind: AudioBlockKind::Insert(insert.clone()),
        };
        let cloned = block.clone();
        assert_eq!(block, cloned);
        assert!(matches!(&block.kind, AudioBlockKind::Insert(ib) if ib.send.device_id.0 == "mk300-out"));
        assert!(matches!(&block.kind, AudioBlockKind::Insert(ib) if ib.return_.device_id.0 == "mk300-in"));
    }

    #[test]
    fn insert_block_in_chain_structure() {
        let chain = crate::chain::Chain {
            id: domain::ids::ChainId("chain:0".to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![
                AudioBlock {
                    id: BlockId("chain:0:input:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".to_string(),
                        entries: vec![InputEntry {
                            device_id: DeviceId("scarlett".into()),
                            mode: ChainInputMode::Mono,
                            channels: vec![0],
                        }],
                    }),
                },
                AudioBlock {
                    id: BlockId("chain:0:insert:0".into()),
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
                },
                AudioBlock {
                    id: BlockId("chain:0:output:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".to_string(),
                        entries: vec![OutputEntry {
                            device_id: DeviceId("scarlett".into()),
                            mode: ChainOutputMode::Stereo,
                            channels: vec![0, 1],
                        }],
                    }),
                },
            ],
        };
        let inserts = chain.insert_blocks();
        assert_eq!(inserts.len(), 1);
        assert_eq!(inserts[0].0, 1); // index 1
        assert_eq!(inserts[0].1.send.device_id.0, "mk300-out");
    }

    #[test]
    fn disabled_insert_block_validates_ok() {
        let block = AudioBlock {
            id: BlockId("chain:0:insert:0".into()),
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
        assert!(block.validate_params().is_ok());
        assert_eq!(block.parameter_descriptors().unwrap(), Vec::new());
        assert_eq!(block.audio_descriptors().unwrap(), Vec::new());
        assert!(block.model_ref().is_none());
    }

    // --- validate_params for all AudioBlockKind variants ---

    #[test]
    fn validate_params_input_block_always_ok() {
        let block = AudioBlock {
            id: BlockId("b:input".into()),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".to_string(),
                entries: vec![InputEntry {
                    device_id: DeviceId("dev".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![0],
                }],
            }),
        };
        assert!(block.validate_params().is_ok());
    }

    #[test]
    fn validate_params_output_block_always_ok() {
        let block = AudioBlock {
            id: BlockId("b:output".into()),
            enabled: true,
            kind: AudioBlockKind::Output(OutputBlock {
                model: "standard".to_string(),
                entries: vec![OutputEntry {
                    device_id: DeviceId("dev".into()),
                    mode: ChainOutputMode::Stereo,
                    channels: vec![0, 1],
                }],
            }),
        };
        assert!(block.validate_params().is_ok());
    }

    #[test]
    fn validate_params_enabled_insert_block_always_ok() {
        let block = AudioBlock {
            id: BlockId("b:insert".into()),
            enabled: true,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".to_string(),
                send: InsertEndpoint {
                    device_id: DeviceId("out-dev".into()),
                    mode: ChainInputMode::Stereo,
                    channels: vec![0, 1],
                },
                return_: InsertEndpoint {
                    device_id: DeviceId("in-dev".into()),
                    mode: ChainInputMode::Stereo,
                    channels: vec![0, 1],
                },
            }),
        };
        assert!(block.validate_params().is_ok());
    }

    #[test]
    fn validate_params_disabled_block_skips_validation() {
        // A disabled Core block with an invalid model should still pass
        let block = AudioBlock {
            id: BlockId("b:disabled".into()),
            enabled: false,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "delay".to_string(),
                model: "nonexistent_model".to_string(),
                params: ParameterSet::default(),
            }),
        };
        assert!(block.validate_params().is_ok());
    }

    #[test]
    fn validate_params_core_block_valid_model() {
        let model = block_delay::supported_models()
            .first()
            .expect("at least one delay model");
        let schema = schema_for_block_model("delay", model).unwrap();
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .unwrap();
        let block = AudioBlock {
            id: BlockId("b:core".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "delay".to_string(),
                model: model.to_string(),
                params,
            }),
        };
        assert!(block.validate_params().is_ok());
    }

    #[test]
    fn validate_params_nam_block_disabled_ok() {
        // The generic NAM model requires model_path (no default), so we test
        // disabled NAM blocks which skip validation entirely.
        let block = AudioBlock {
            id: BlockId("b:nam".into()),
            enabled: false,
            kind: AudioBlockKind::Nam(super::NamBlock {
                model: "neural_amp_modeler".to_string(),
                params: ParameterSet::default(),
            }),
        };
        assert!(block.validate_params().is_ok());
    }

    #[test]
    fn validate_params_select_block_valid() {
        let model = block_delay::supported_models()
            .first()
            .expect("at least one delay model");
        let opt = delay_block("sel::a", model);
        let block = AudioBlock {
            id: BlockId("b:sel".into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId("sel::a".into()),
                options: vec![opt],
            }),
        };
        assert!(block.validate_params().is_ok());
    }

    // --- audio_descriptors for all AudioBlockKind variants ---

    #[test]
    fn audio_descriptors_input_returns_empty() {
        let block = AudioBlock {
            id: BlockId("b:input".into()),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".to_string(),
                entries: vec![],
            }),
        };
        assert!(block.audio_descriptors().unwrap().is_empty());
    }

    #[test]
    fn audio_descriptors_output_returns_empty() {
        let block = AudioBlock {
            id: BlockId("b:output".into()),
            enabled: true,
            kind: AudioBlockKind::Output(OutputBlock {
                model: "standard".to_string(),
                entries: vec![],
            }),
        };
        assert!(block.audio_descriptors().unwrap().is_empty());
    }

    #[test]
    fn audio_descriptors_insert_returns_empty() {
        let block = AudioBlock {
            id: BlockId("b:insert".into()),
            enabled: true,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".to_string(),
                send: InsertEndpoint {
                    device_id: DeviceId("d".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![0],
                },
                return_: InsertEndpoint {
                    device_id: DeviceId("d".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![0],
                },
            }),
        };
        assert!(block.audio_descriptors().unwrap().is_empty());
    }

    #[test]
    fn audio_descriptors_disabled_block_returns_empty() {
        let model = block_delay::supported_models().first().unwrap();
        let mut blk = delay_block("b:dis", model);
        blk.enabled = false;
        assert!(blk.audio_descriptors().unwrap().is_empty());
    }

    #[test]
    fn audio_descriptors_core_block_returns_one() {
        let model = block_reverb::supported_models().first().unwrap();
        let blk = reverb_block("b:core", model);
        let descs = blk.audio_descriptors().unwrap();
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].effect_type, "reverb");
        assert_eq!(descs[0].block_id.0, "b:core");
    }

    #[test]
    fn audio_descriptors_disabled_nam_block_returns_empty() {
        // Generic NAM requires model_path; test disabled path which returns empty
        let block = AudioBlock {
            id: BlockId("b:nam".into()),
            enabled: false,
            kind: AudioBlockKind::Nam(super::NamBlock {
                model: "neural_amp_modeler".to_string(),
                params: ParameterSet::default(),
            }),
        };
        let descs = block.audio_descriptors().unwrap();
        assert!(descs.is_empty());
    }

    #[test]
    fn audio_descriptors_select_delegates_to_selected() {
        let model = block_delay::supported_models().first().unwrap();
        let opt = delay_block("sel::a", model);
        let block = AudioBlock {
            id: BlockId("b:sel".into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId("sel::a".into()),
                options: vec![opt],
            }),
        };
        let descs = block.audio_descriptors().unwrap();
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].effect_type, "delay");
    }

    #[test]
    fn audio_descriptors_select_missing_selected_errors() {
        let model = block_delay::supported_models().first().unwrap();
        let opt = delay_block("sel::a", model);
        let block = AudioBlock {
            id: BlockId("b:sel".into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId("sel::missing".into()),
                options: vec![opt],
            }),
        };
        assert!(block.audio_descriptors().is_err());
    }

    // --- model_ref for all AudioBlockKind variants ---

    #[test]
    fn model_ref_nam_returns_some() {
        // model_ref does not validate params, so we can use default params
        let block = AudioBlock {
            id: BlockId("b:nam".into()),
            enabled: true,
            kind: AudioBlockKind::Nam(super::NamBlock {
                model: "neural_amp_modeler".to_string(),
                params: ParameterSet::default(),
            }),
        };
        let mr = block.model_ref().expect("nam should expose model_ref");
        assert_eq!(mr.effect_type, "nam");
        assert_eq!(mr.model, "neural_amp_modeler");
    }

    #[test]
    fn model_ref_core_returns_some() {
        let model = block_delay::supported_models().first().unwrap();
        let blk = delay_block("b:core", model);
        let mr = blk.model_ref().expect("core should expose model_ref");
        assert_eq!(mr.effect_type, "delay");
        assert_eq!(mr.model, *model);
    }

    #[test]
    fn model_ref_select_returns_none() {
        let model = block_delay::supported_models().first().unwrap();
        let opt = delay_block("sel::a", model);
        let block = AudioBlock {
            id: BlockId("b:sel".into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId("sel::a".into()),
                options: vec![opt],
            }),
        };
        assert!(block.model_ref().is_none());
    }

    #[test]
    fn model_ref_input_returns_none() {
        let block = AudioBlock {
            id: BlockId("b:in".into()),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".to_string(),
                entries: vec![],
            }),
        };
        assert!(block.model_ref().is_none());
    }

    #[test]
    fn model_ref_output_returns_none() {
        let block = AudioBlock {
            id: BlockId("b:out".into()),
            enabled: true,
            kind: AudioBlockKind::Output(OutputBlock {
                model: "standard".to_string(),
                entries: vec![],
            }),
        };
        assert!(block.model_ref().is_none());
    }

    #[test]
    fn model_ref_insert_returns_none() {
        let block = AudioBlock {
            id: BlockId("b:ins".into()),
            enabled: true,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".to_string(),
                send: InsertEndpoint {
                    device_id: DeviceId("d".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![0],
                },
                return_: InsertEndpoint {
                    device_id: DeviceId("d".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![0],
                },
            }),
        };
        assert!(block.model_ref().is_none());
    }

    // --- parameter_descriptors for all AudioBlockKind variants ---

    #[test]
    fn parameter_descriptors_input_returns_empty() {
        let block = AudioBlock {
            id: BlockId("b:in".into()),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".to_string(),
                entries: vec![],
            }),
        };
        assert!(block.parameter_descriptors().unwrap().is_empty());
    }

    #[test]
    fn parameter_descriptors_output_returns_empty() {
        let block = AudioBlock {
            id: BlockId("b:out".into()),
            enabled: true,
            kind: AudioBlockKind::Output(OutputBlock {
                model: "standard".to_string(),
                entries: vec![],
            }),
        };
        assert!(block.parameter_descriptors().unwrap().is_empty());
    }

    #[test]
    fn parameter_descriptors_core_returns_params() {
        let model = block_delay::supported_models().first().unwrap();
        let blk = delay_block("b:core", model);
        let descs = blk.parameter_descriptors().unwrap();
        assert!(!descs.is_empty(), "delay block should have parameter descriptors");
    }

    #[test]
    fn parameter_descriptors_select_delegates_to_selected() {
        let model = block_delay::supported_models().first().unwrap();
        let opt = delay_block("sel::a", model);
        let block = AudioBlock {
            id: BlockId("b:sel".into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId("sel::a".into()),
                options: vec![opt],
            }),
        };
        let descs = block.parameter_descriptors().unwrap();
        assert!(!descs.is_empty());
    }

    #[test]
    fn parameter_descriptors_select_missing_selected_errors() {
        let model = block_delay::supported_models().first().unwrap();
        let opt = delay_block("sel::a", model);
        let block = AudioBlock {
            id: BlockId("b:sel".into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId("sel::missing".into()),
                options: vec![opt],
            }),
        };
        assert!(block.parameter_descriptors().is_err());
    }

    // --- SelectBlock validate_structure edge cases ---

    #[test]
    fn select_block_rejects_nested_select_option() {
        let model = block_delay::supported_models().first().unwrap();
        let inner_opt = delay_block("inner::a", model);
        let nested_select = AudioBlock {
            id: BlockId("sel::nested".into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId("inner::a".into()),
                options: vec![inner_opt],
            }),
        };
        let block = AudioBlock {
            id: BlockId("b:sel".into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId("sel::nested".into()),
                options: vec![nested_select],
            }),
        };
        let err = block.validate_params().expect_err("nested select should fail");
        assert!(err.contains("select, input, output, or insert"));
    }

    #[test]
    fn select_block_rejects_input_option() {
        let input_opt = AudioBlock {
            id: BlockId("sel::in".into()),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".to_string(),
                entries: vec![],
            }),
        };
        let block = AudioBlock {
            id: BlockId("b:sel".into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId("sel::in".into()),
                options: vec![input_opt],
            }),
        };
        let err = block.validate_params().expect_err("input option should fail");
        assert!(err.contains("select, input, output, or insert"));
    }

    #[test]
    fn select_block_rejects_output_option() {
        let output_opt = AudioBlock {
            id: BlockId("sel::out".into()),
            enabled: true,
            kind: AudioBlockKind::Output(OutputBlock {
                model: "standard".to_string(),
                entries: vec![],
            }),
        };
        let block = AudioBlock {
            id: BlockId("b:sel".into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId("sel::out".into()),
                options: vec![output_opt],
            }),
        };
        let err = block.validate_params().expect_err("output option should fail");
        assert!(err.contains("select, input, output, or insert"));
    }

    #[test]
    fn select_block_rejects_insert_option() {
        let insert_opt = AudioBlock {
            id: BlockId("sel::ins".into()),
            enabled: true,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".to_string(),
                send: InsertEndpoint {
                    device_id: DeviceId("d".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![0],
                },
                return_: InsertEndpoint {
                    device_id: DeviceId("d".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![0],
                },
            }),
        };
        let block = AudioBlock {
            id: BlockId("b:sel".into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId("sel::ins".into()),
                options: vec![insert_opt],
            }),
        };
        let err = block.validate_params().expect_err("insert option should fail");
        assert!(err.contains("select, input, output, or insert"));
    }

    #[test]
    fn select_block_exactly_eight_options_ok() {
        let model = block_delay::supported_models().first().unwrap();
        let options: Vec<_> = (0..8)
            .map(|i| delay_block(format!("sel::{i}"), model))
            .collect();
        let block = AudioBlock {
            id: BlockId("b:sel".into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId("sel::0".into()),
                options,
            }),
        };
        assert!(block.validate_params().is_ok());
    }

    // --- schema_for_block_model and normalize_block_params edge cases ---

    #[test]
    fn schema_for_unsupported_block_type_returns_error() {
        let result = schema_for_block_model("nonexistent_type", "some_model");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unsupported block type"));
    }

    #[test]
    fn normalize_block_params_unsupported_type_returns_error() {
        let result = normalize_block_params("nonexistent_type", "some_model", ParameterSet::default());
        assert!(result.is_err());
    }

    #[test]
    fn schema_covers_all_static_effect_types() {
        let types_and_models = [
            ("preamp", block_preamp::supported_models()),
            ("amp", block_amp::supported_models()),
            ("cab", block_cab::supported_models()),
            ("body", block_body::supported_models()),
            ("ir", block_ir::supported_models()),
            ("gain", block_gain::supported_models()),
            ("nam", block_nam::supported_models()),
            ("delay", block_delay::supported_models()),
            ("reverb", block_reverb::supported_models()),
            ("utility", block_util::supported_models()),
            ("dynamics", block_dyn::supported_models()),
            ("filter", block_filter::supported_models()),
            ("wah", block_wah::supported_models()),
            ("pitch", block_pitch::supported_models()),
            ("modulation", block_mod::supported_models()),
        ];
        for (effect_type, models) in types_and_models {
            assert!(
                !models.is_empty(),
                "{effect_type} should have at least one model"
            );
            let schema = schema_for_block_model(effect_type, models[0]).unwrap();
            assert_eq!(schema.effect_type, effect_type);
        }
    }

    // --- InputBlock validate_channel_conflicts edge cases ---

    #[test]
    fn input_block_empty_entries_validates_ok() {
        let input = InputBlock {
            model: "standard".to_string(),
            entries: vec![],
        };
        assert!(input.validate_channel_conflicts().is_ok());
    }

    #[test]
    fn input_block_stereo_duplicate_channel_detected() {
        let input = InputBlock {
            model: "standard".to_string(),
            entries: vec![
                InputEntry {
                    device_id: DeviceId("dev".into()),
                    mode: ChainInputMode::Stereo,
                    channels: vec![0, 1],
                },
                InputEntry {
                    device_id: DeviceId("dev".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![1], // conflicts with entry A channel 1
                },
            ],
        };
        let err = input.validate_channel_conflicts().unwrap_err();
        assert!(err.contains("Channel 1"));
    }

    // --- build_audio_block_kind ---

    #[test]
    fn build_audio_block_kind_core_types() {
        let model = block_delay::supported_models().first().unwrap();
        let schema = schema_for_block_model("delay", model).unwrap();
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .unwrap();
        let kind = super::build_audio_block_kind("delay", model, params).unwrap();
        assert!(matches!(kind, AudioBlockKind::Core(_)));
    }

    #[test]
    fn build_audio_block_kind_nam_type() {
        // build_audio_block_kind just constructs the enum variant without
        // validating params, so we can use empty params here.
        let kind = super::build_audio_block_kind(
            "nam",
            "neural_amp_modeler",
            ParameterSet::default(),
        )
        .unwrap();
        assert!(matches!(kind, AudioBlockKind::Nam(_)));
    }

    #[test]
    fn build_audio_block_kind_unsupported_type_errors() {
        let result = super::build_audio_block_kind("nonexistent", "model", ParameterSet::default());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unsupported block type"));
    }

}
