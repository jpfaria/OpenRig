#![allow(dead_code)]

use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use serde::{Deserialize, Serialize};
use stage_amp_combo::{amp_combo_model_schema, validate_amp_combo_params};
use stage_amp_head::{amp_head_model_schema, validate_amp_head_params};
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
    CoreNam(CoreNamBlock),
    Core(CoreBlock),
    Select(SelectBlock),
}

define_model_block!(NamBlock);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreNamBlock {
    pub model_id: String,
    pub ir_id: Option<String>,
}

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
    IrLoader(IrLoaderBlock),

    Drive(DriveBlock),
    Compressor(CompressorBlock),
    Gate(GateBlock),
    Eq(EqBlock),
    Filter(FilterBlock),
    Wah(WahBlock),
    Pitch(PitchBlock),

    Chorus(ChorusBlock),
    Flanger(FlangerBlock),
    Phaser(PhaserBlock),
    Tremolo(TremoloBlock),
    Rotary(RotaryBlock),

    Delay(DelayBlock),
    Reverb(ReverbBlock),

    Mixer(MixerBlock),
    Split(SplitBlock),
    Merge(MergeBlock),
    Send(SendBlock),
    Return(ReturnBlock),
    VolumePan(VolumePanBlock),

    Looper(LooperBlock),
    Tuner(TunerBlock),
    Synth(SynthBlock),
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
define_model_block!(IrLoaderBlock);
define_model_block!(DriveBlock);
define_model_block!(CompressorBlock);
define_model_block!(GateBlock);
define_model_block!(EqBlock);
define_model_block!(FilterBlock);
define_model_block!(WahBlock);
define_model_block!(PitchBlock);
define_model_block!(ChorusBlock);
define_model_block!(FlangerBlock);
define_model_block!(PhaserBlock);
define_model_block!(TremoloBlock);
define_model_block!(RotaryBlock);
define_model_block!(DelayBlock);
define_model_block!(ReverbBlock);
define_model_block!(MixerBlock);
define_model_block!(SplitBlock);
define_model_block!(MergeBlock);
define_model_block!(SendBlock);
define_model_block!(ReturnBlock);
define_model_block!(VolumePanBlock);
define_model_block!(LooperBlock);
define_model_block!(TunerBlock);
define_model_block!(SynthBlock);

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
            AudioBlockKind::CoreNam(_) => Ok(()),
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
            AudioBlockKind::CoreNam(_) => Ok(Vec::new()),
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
            AudioBlockKind::CoreNam(_) => Ok(Vec::new()),
        }
    }
}

const fn default_enabled() -> bool {
    true
}

impl CoreBlock {
    fn validate_params(&self) -> Result<(), String> {
        match &self.kind {
            CoreBlockKind::AmpHead(stage) => {
                normalize_block_params("amp_head", &stage.model, stage.params.clone())?;
                Ok(())
            }
            CoreBlockKind::AmpCombo(stage) => {
                normalize_block_params("amp_combo", &stage.model, stage.params.clone())?;
                Ok(())
            }
            CoreBlockKind::FullRig(stage) => {
                normalize_block_params("full_rig", &stage.model, stage.params.clone())?;
                Ok(())
            }
            CoreBlockKind::Drive(stage) => {
                normalize_block_params("drive", &stage.model, stage.params.clone())?;
                Ok(())
            }
            CoreBlockKind::Delay(stage) => {
                normalize_block_params("delay", &stage.model, stage.params.clone())?;
                Ok(())
            }
            CoreBlockKind::Reverb(stage) => {
                normalize_block_params("reverb", &stage.model, stage.params.clone())?;
                Ok(())
            }
            CoreBlockKind::Tuner(stage) => {
                normalize_block_params("tuner", &stage.model, stage.params.clone())?;
                Ok(())
            }
            CoreBlockKind::Compressor(stage) => {
                normalize_block_params("compressor", &stage.model, stage.params.clone())?;
                Ok(())
            }
            CoreBlockKind::Gate(stage) => {
                normalize_block_params("gate", &stage.model, stage.params.clone())?;
                Ok(())
            }
            CoreBlockKind::Eq(stage) => {
                normalize_block_params("eq", &stage.model, stage.params.clone())?;
                Ok(())
            }
            CoreBlockKind::Tremolo(stage) => {
                normalize_block_params("tremolo", &stage.model, stage.params.clone())?;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn parameter_descriptors(
        &self,
        block_id: &BlockId,
    ) -> Result<Vec<BlockParameterDescriptor>, String> {
        match &self.kind {
            CoreBlockKind::AmpHead(stage) => {
                describe_block_params(block_id, "amp_head", &stage.model, &stage.params)
            }
            CoreBlockKind::AmpCombo(stage) => {
                describe_block_params(block_id, "amp_combo", &stage.model, &stage.params)
            }
            CoreBlockKind::FullRig(stage) => {
                describe_block_params(block_id, "full_rig", &stage.model, &stage.params)
            }
            CoreBlockKind::Drive(stage) => {
                describe_block_params(block_id, "drive", &stage.model, &stage.params)
            }
            CoreBlockKind::Delay(stage) => {
                describe_block_params(block_id, "delay", &stage.model, &stage.params)
            }
            CoreBlockKind::Reverb(stage) => {
                describe_block_params(block_id, "reverb", &stage.model, &stage.params)
            }
            CoreBlockKind::Tuner(stage) => {
                describe_block_params(block_id, "tuner", &stage.model, &stage.params)
            }
            CoreBlockKind::Compressor(stage) => {
                describe_block_params(block_id, "compressor", &stage.model, &stage.params)
            }
            CoreBlockKind::Gate(stage) => {
                describe_block_params(block_id, "gate", &stage.model, &stage.params)
            }
            CoreBlockKind::Eq(stage) => {
                describe_block_params(block_id, "eq", &stage.model, &stage.params)
            }
            CoreBlockKind::Tremolo(stage) => {
                describe_block_params(block_id, "tremolo", &stage.model, &stage.params)
            }
            _ => Ok(Vec::new()),
        }
    }

    fn audio_descriptors(&self, block_id: &BlockId) -> Result<Vec<BlockAudioDescriptor>, String> {
        match &self.kind {
            CoreBlockKind::AmpHead(stage) => {
                Ok(vec![describe_block_audio(block_id, "amp_head", &stage.model)?])
            }
            CoreBlockKind::AmpCombo(stage) => {
                Ok(vec![describe_block_audio(block_id, "amp_combo", &stage.model)?])
            }
            CoreBlockKind::FullRig(stage) => {
                Ok(vec![describe_block_audio(block_id, "full_rig", &stage.model)?])
            }
            CoreBlockKind::Drive(stage) => {
                Ok(vec![describe_block_audio(block_id, "drive", &stage.model)?])
            }
            CoreBlockKind::Delay(stage) => {
                Ok(vec![describe_block_audio(block_id, "delay", &stage.model)?])
            }
            CoreBlockKind::Reverb(stage) => Ok(vec![describe_block_audio(
                block_id,
                "reverb",
                &stage.model,
            )?]),
            CoreBlockKind::Tuner(stage) => {
                Ok(vec![describe_block_audio(block_id, "tuner", &stage.model)?])
            }
            CoreBlockKind::Compressor(stage) => Ok(vec![describe_block_audio(
                block_id,
                "compressor",
                &stage.model,
            )?]),
            CoreBlockKind::Gate(stage) => {
                Ok(vec![describe_block_audio(block_id, "gate", &stage.model)?])
            }
            CoreBlockKind::Eq(stage) => {
                Ok(vec![describe_block_audio(block_id, "eq", &stage.model)?])
            }
            CoreBlockKind::Tremolo(stage) => Ok(vec![describe_block_audio(
                block_id,
                "tremolo",
                &stage.model,
            )?]),
            _ => Ok(Vec::new()),
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
    if effect_type == "amp_head" {
        validate_amp_head_params(model, &normalized).map_err(|error| error.to_string())?;
    }
    if effect_type == "amp_combo" {
        validate_amp_combo_params(model, &normalized).map_err(|error| error.to_string())?;
    }
    if effect_type == "full_rig" {
        validate_full_rig_params(model, &normalized).map_err(|error| error.to_string())?;
    }
    if effect_type == "drive" {
        validate_drive_params(model, &normalized).map_err(|error| error.to_string())?;
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
