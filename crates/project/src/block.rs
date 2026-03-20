use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use serde::{Deserialize, Serialize};
use stage_amp_combo::{amp_combo_model_schema, validate_amp_combo_params};
use stage_amp_head::{amp_head_model_schema, validate_amp_head_params};
use stage_cab::{cab_model_schema, validate_cab_params};
use stage_core::ModelAudioMode;
use stage_delay::delay_model_schema;
use stage_dyn::compressor_model_schema;
use stage_dyn::gate_model_schema;
use stage_filter::eq_model_schema;
use stage_full_rig::{full_rig_model_schema, validate_full_rig_params};
use stage_gain::{drive_model_schema, validate_drive_params};
use stage_mod::tremolo_model_schema;
use stage_nam::nam_model_schema;
use stage_reverb::reverb_model_schema;
use stage_util::tuner_model_schema;

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
    Drive(DriveBlock),
    Compressor(CompressorBlock),
    Gate(GateBlock),
    Eq(EqBlock),
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
define_model_block!(DriveBlock);
define_model_block!(CompressorBlock);
define_model_block!(GateBlock);
define_model_block!(EqBlock);
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
                effect_type: "tuner",
                model: &stage.model,
                params: &stage.params,
            },
            CoreBlockKind::Compressor(stage) => BlockModelRef {
                effect_type: "compressor",
                model: &stage.model,
                params: &stage.params,
            },
            CoreBlockKind::Gate(stage) => BlockModelRef {
                effect_type: "gate",
                model: &stage.model,
                params: &stage.params,
            },
            CoreBlockKind::Eq(stage) => BlockModelRef {
                effect_type: "eq",
                model: &stage.model,
                params: &stage.params,
            },
            CoreBlockKind::Tremolo(stage) => BlockModelRef {
                effect_type: "tremolo",
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
        "drive" => validate_drive_params(model, &normalized).map_err(|error| error.to_string())?,
        _ => {}
    }
    Ok(normalized)
}

pub fn schema_for_block_model(
    effect_type: &str,
    model: &str,
) -> Result<ModelParameterSchema, String> {
    match effect_type {
        "amp" | "amp_head" => amp_head_model_schema(model).map_err(|error| error.to_string()),
        "amp_combo" => amp_combo_model_schema(model).map_err(|error| error.to_string()),
        "full_rig" => full_rig_model_schema(model).map_err(|error| error.to_string()),
        "cab" => cab_model_schema(model).map_err(|error| error.to_string()),
        "drive" => drive_model_schema(model).map_err(|error| error.to_string()),
        "nam" => nam_model_schema(model).map_err(|error| error.to_string()),
        "delay" => delay_model_schema(model).map_err(|error| error.to_string()),
        "reverb" => reverb_model_schema(model).map_err(|error| error.to_string()),
        "tuner" => tuner_model_schema(model).map_err(|error| error.to_string()),
        "compressor" => compressor_model_schema(model).map_err(|error| error.to_string()),
        "gate" => gate_model_schema(model).map_err(|error| error.to_string()),
        "eq" => eq_model_schema(model).map_err(|error| error.to_string()),
        "tremolo" => tremolo_model_schema(model).map_err(|error| error.to_string()),
        other => Err(format!("unsupported block type '{}'", other)),
    }
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

    #[test]
    fn cab_schema_is_available_through_project_contract() {
        let schema =
            schema_for_block_model("cab", "marshall_4x12_v30").expect("cab schema should exist");

        assert_eq!(schema.effect_type, "cab");
        assert_eq!(schema.model, "marshall_4x12_v30");
        assert_eq!(schema.parameters.len(), 1);
        assert_eq!(schema.parameters[0].path, "capture");
    }

    #[test]
    fn cab_params_normalize_with_default_capture() {
        let normalized = normalize_block_params(
            "cab",
            "marshall_4x12_v30",
            crate::param::ParameterSet::default(),
        )
        .expect("cab params should normalize");

        assert_eq!(normalized.get_string("capture"), Some("ev_mix_b"));
    }

    #[test]
    fn native_amp_head_schema_is_available_through_project_contract() {
        let schema =
            schema_for_block_model("amp_head", "brit_crunch_head").expect("schema should exist");

        assert_eq!(schema.effect_type, "amp");
        assert_eq!(schema.model, "brit_crunch_head");
        assert_eq!(schema.parameters.len(), 11);
    }

    #[test]
    fn native_cab_schema_is_available_through_project_contract() {
        let schema = schema_for_block_model("cab", "brit_4x12_cab").expect("schema should exist");

        assert_eq!(schema.effect_type, "cab");
        assert_eq!(schema.model, "brit_4x12_cab");
        assert_eq!(schema.parameters.len(), 8);
    }

    #[test]
    fn native_amp_combo_schema_is_available_through_project_contract() {
        let schema = schema_for_block_model("amp_combo", "blackface_clean_combo")
            .expect("schema should exist");

        assert_eq!(schema.effect_type, "amp_combo");
        assert_eq!(schema.model, "blackface_clean_combo");
        assert_eq!(schema.parameters.len(), 10);
    }
}
