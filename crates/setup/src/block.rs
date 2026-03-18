#![allow(dead_code)]

use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use serde::{Deserialize, Serialize};
use stage_amp_nam::nam_model_schema;
use stage_core::ModelAudioMode;
use stage_delay_digital::delay_model_schema;
use stage_dyn_compressor::compressor_model_schema;
use stage_dyn_gate::gate_model_schema;
use stage_filter_eq::eq_model_schema;
use stage_mod_tremolo::tremolo_model_schema;
use stage_reverb_plate::reverb_model_schema;
use stage_util_tuner::tuner_model_schema;

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
    Amp(AmpBlock),
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

define_model_block!(AmpBlock);
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

impl CoreBlock {
    fn validate_params(&self) -> Result<(), String> {
        match &self.kind {
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
    params.normalized_against(&schema)
}

pub fn schema_for_block_model(
    effect_type: &str,
    model: &str,
) -> Result<ModelParameterSchema, String> {
    match effect_type {
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
