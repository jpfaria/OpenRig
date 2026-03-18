#![allow(dead_code)]

use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use serde::{Deserialize, Serialize};

use crate::param::{
    bool_parameter, file_path_parameter, float_parameter, BlockParameterDescriptor,
    ModelParameterSchema, ParameterSet, ParameterUnit,
};

pub const NAM_MODEL_NEURAL_AMP_MODELER: &str = "neural_amp_modeler";
pub const DELAY_MODEL_DIGITAL_BASIC: &str = "digital_basic";
pub const REVERB_MODEL_PLATE_FOUNDATION: &str = "plate_foundation";
pub const TUNER_MODEL_CHROMATIC_BASIC: &str = "chromatic_basic";
pub const COMPRESSOR_MODEL_STUDIO_CLEAN: &str = "studio_clean";
pub const GATE_MODEL_NOISE_GATE_BASIC: &str = "noise_gate_basic";
pub const EQ_MODEL_THREE_BAND_BASIC: &str = "three_band_basic";
pub const TREMOLO_MODEL_SINE: &str = "sine_tremolo";

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
        "nam" => nam_model_schema(model),
        "delay" => delay_model_schema(model),
        "reverb" => reverb_model_schema(model),
        "tuner" => tuner_model_schema(model),
        "compressor" => compressor_model_schema(model),
        "gate" => gate_model_schema(model),
        "eq" => eq_model_schema(model),
        "tremolo" => tremolo_model_schema(model),
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
            spec.materialize(block_id, effect_type, model, current_value)
        })
        .collect())
}

fn nam_model_schema(model: &str) -> Result<ModelParameterSchema, String> {
    match model {
        NAM_MODEL_NEURAL_AMP_MODELER => Ok(ModelParameterSchema {
            effect_type: "nam".to_string(),
            model: model.to_string(),
            display_name: "Neural Amp Modeler".to_string(),
            parameters: vec![
                file_path_parameter("model_path", "Model", None, None, &["nam"], false),
                file_path_parameter(
                    "ir_path",
                    "Impulse Response",
                    None,
                    Some(ParameterValue::Null),
                    &["wav"],
                    true,
                ),
                float_parameter(
                    "input_db",
                    "Input",
                    None,
                    Some(0.0),
                    -24.0,
                    24.0,
                    0.1,
                    ParameterUnit::Decibels,
                ),
                float_parameter(
                    "output_db",
                    "Output",
                    None,
                    Some(0.0),
                    -24.0,
                    24.0,
                    0.1,
                    ParameterUnit::Decibels,
                ),
                bool_parameter(
                    "noise_gate.enabled",
                    "Enabled",
                    Some("Noise Gate"),
                    Some(true),
                ),
                float_parameter(
                    "noise_gate.threshold_db",
                    "Threshold",
                    Some("Noise Gate"),
                    Some(-80.0),
                    -96.0,
                    0.0,
                    0.1,
                    ParameterUnit::Decibels,
                ),
                bool_parameter("eq.enabled", "Enabled", Some("EQ"), Some(true)),
                float_parameter(
                    "eq.bass",
                    "Bass",
                    Some("EQ"),
                    Some(5.0),
                    0.0,
                    10.0,
                    0.1,
                    ParameterUnit::None,
                ),
                float_parameter(
                    "eq.middle",
                    "Middle",
                    Some("EQ"),
                    Some(5.0),
                    0.0,
                    10.0,
                    0.1,
                    ParameterUnit::None,
                ),
                float_parameter(
                    "eq.treble",
                    "Treble",
                    Some("EQ"),
                    Some(5.0),
                    0.0,
                    10.0,
                    0.1,
                    ParameterUnit::None,
                ),
                bool_parameter("ir_enabled", "IR Enabled", None, Some(true)),
            ],
        }),
        other => Err(format!("unsupported nam model '{}'", other)),
    }
}

fn delay_model_schema(model: &str) -> Result<ModelParameterSchema, String> {
    match model {
        DELAY_MODEL_DIGITAL_BASIC | "native_digital" | "rust_style_digital" | "digital" => {
            Ok(ModelParameterSchema {
                effect_type: "delay".to_string(),
                model: model.to_string(),
                display_name: "Digital Basic Delay".to_string(),
                parameters: vec![
                    float_parameter(
                        "time_ms",
                        "Time",
                        None,
                        Some(380.0),
                        0.0,
                        2_000.0,
                        1.0,
                        ParameterUnit::Milliseconds,
                    ),
                    float_parameter(
                        "feedback",
                        "Feedback",
                        None,
                        Some(0.35),
                        0.0,
                        0.95,
                        0.01,
                        ParameterUnit::None,
                    ),
                    float_parameter(
                        "mix",
                        "Mix",
                        None,
                        Some(0.3),
                        0.0,
                        1.0,
                        0.01,
                        ParameterUnit::None,
                    ),
                ],
            })
        }
        other => Err(format!("unsupported delay model '{}'", other)),
    }
}

fn reverb_model_schema(model: &str) -> Result<ModelParameterSchema, String> {
    match model {
        REVERB_MODEL_PLATE_FOUNDATION | "plate" => Ok(ModelParameterSchema {
            effect_type: "reverb".to_string(),
            model: model.to_string(),
            display_name: "Plate Foundation Reverb".to_string(),
            parameters: vec![
                float_parameter(
                    "room_size",
                    "Room Size",
                    None,
                    Some(0.45),
                    0.0,
                    1.0,
                    0.01,
                    ParameterUnit::None,
                ),
                float_parameter(
                    "damping",
                    "Damping",
                    None,
                    Some(0.35),
                    0.0,
                    1.0,
                    0.01,
                    ParameterUnit::None,
                ),
                float_parameter(
                    "mix",
                    "Mix",
                    None,
                    Some(0.25),
                    0.0,
                    1.0,
                    0.01,
                    ParameterUnit::None,
                ),
            ],
        }),
        other => Err(format!("unsupported reverb model '{}'", other)),
    }
}

fn tuner_model_schema(model: &str) -> Result<ModelParameterSchema, String> {
    match model {
        TUNER_MODEL_CHROMATIC_BASIC | "chromatic" => Ok(ModelParameterSchema {
            effect_type: "tuner".to_string(),
            model: model.to_string(),
            display_name: "Chromatic Tuner".to_string(),
            parameters: vec![float_parameter(
                "reference_hz",
                "Reference",
                None,
                Some(440.0),
                400.0,
                480.0,
                1.0,
                ParameterUnit::Hertz,
            )],
        }),
        other => Err(format!("unsupported tuner model '{}'", other)),
    }
}

fn compressor_model_schema(model: &str) -> Result<ModelParameterSchema, String> {
    match model {
        COMPRESSOR_MODEL_STUDIO_CLEAN => Ok(ModelParameterSchema {
            effect_type: "compressor".to_string(),
            model: model.to_string(),
            display_name: "Studio Clean Compressor".to_string(),
            parameters: vec![
                float_parameter(
                    "threshold",
                    "Threshold",
                    None,
                    Some(-18.0),
                    -60.0,
                    0.0,
                    0.1,
                    ParameterUnit::Decibels,
                ),
                float_parameter(
                    "ratio",
                    "Ratio",
                    None,
                    Some(4.0),
                    1.0,
                    20.0,
                    0.1,
                    ParameterUnit::Ratio,
                ),
                float_parameter(
                    "attack_ms",
                    "Attack",
                    None,
                    Some(10.0),
                    0.1,
                    200.0,
                    0.1,
                    ParameterUnit::Milliseconds,
                ),
                float_parameter(
                    "release_ms",
                    "Release",
                    None,
                    Some(80.0),
                    1.0,
                    500.0,
                    0.1,
                    ParameterUnit::Milliseconds,
                ),
                float_parameter(
                    "makeup_gain_db",
                    "Makeup Gain",
                    None,
                    Some(0.0),
                    -24.0,
                    24.0,
                    0.1,
                    ParameterUnit::Decibels,
                ),
                float_parameter(
                    "mix",
                    "Mix",
                    None,
                    Some(1.0),
                    0.0,
                    1.0,
                    0.01,
                    ParameterUnit::None,
                ),
            ],
        }),
        other => Err(format!("unsupported compressor model '{}'", other)),
    }
}

fn gate_model_schema(model: &str) -> Result<ModelParameterSchema, String> {
    match model {
        GATE_MODEL_NOISE_GATE_BASIC => Ok(ModelParameterSchema {
            effect_type: "gate".to_string(),
            model: model.to_string(),
            display_name: "Noise Gate".to_string(),
            parameters: vec![
                float_parameter(
                    "threshold",
                    "Threshold",
                    None,
                    Some(-60.0),
                    -96.0,
                    0.0,
                    0.1,
                    ParameterUnit::Decibels,
                ),
                float_parameter(
                    "attack_ms",
                    "Attack",
                    None,
                    Some(5.0),
                    0.1,
                    100.0,
                    0.1,
                    ParameterUnit::Milliseconds,
                ),
                float_parameter(
                    "release_ms",
                    "Release",
                    None,
                    Some(50.0),
                    1.0,
                    500.0,
                    0.1,
                    ParameterUnit::Milliseconds,
                ),
            ],
        }),
        other => Err(format!("unsupported gate model '{}'", other)),
    }
}

fn eq_model_schema(model: &str) -> Result<ModelParameterSchema, String> {
    match model {
        EQ_MODEL_THREE_BAND_BASIC => Ok(ModelParameterSchema {
            effect_type: "eq".to_string(),
            model: model.to_string(),
            display_name: "Three Band EQ".to_string(),
            parameters: vec![
                float_parameter(
                    "low_gain_db",
                    "Low",
                    None,
                    Some(0.0),
                    -24.0,
                    24.0,
                    0.1,
                    ParameterUnit::Decibels,
                ),
                float_parameter(
                    "mid_gain_db",
                    "Mid",
                    None,
                    Some(0.0),
                    -24.0,
                    24.0,
                    0.1,
                    ParameterUnit::Decibels,
                ),
                float_parameter(
                    "high_gain_db",
                    "High",
                    None,
                    Some(0.0),
                    -24.0,
                    24.0,
                    0.1,
                    ParameterUnit::Decibels,
                ),
            ],
        }),
        other => Err(format!("unsupported eq model '{}'", other)),
    }
}

fn tremolo_model_schema(model: &str) -> Result<ModelParameterSchema, String> {
    match model {
        TREMOLO_MODEL_SINE => Ok(ModelParameterSchema {
            effect_type: "tremolo".to_string(),
            model: model.to_string(),
            display_name: "Sine Tremolo".to_string(),
            parameters: vec![
                float_parameter(
                    "rate_hz",
                    "Rate",
                    None,
                    Some(4.0),
                    0.1,
                    20.0,
                    0.1,
                    ParameterUnit::Hertz,
                ),
                float_parameter(
                    "depth",
                    "Depth",
                    None,
                    Some(0.5),
                    0.0,
                    1.0,
                    0.01,
                    ParameterUnit::None,
                ),
            ],
        }),
        other => Err(format!("unsupported tremolo model '{}'", other)),
    }
}
