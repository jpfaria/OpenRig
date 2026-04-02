use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_fomp_cs_phaser";
pub const DISPLAY_NAME: &str = "CS Phaser";
const BRAND: &str = "fomp";

const PLUGIN_URI: &str = "http://drobilla.net/plugins/fomp/cs_phaser1";
const PLUGIN_DIR: &str = "fomp";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "cs_phaser.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "cs_phaser.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "cs_phaser.dll";

// LV2 port indices (from TTL)
const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;
const PORT_FM_CV: usize = 2;
const PORT_EXP_FM_CV: usize = 3;
const PORT_LIN_FM_CV: usize = 4;
const PORT_INPUT_GAIN: usize = 5;
const PORT_SECTIONS: usize = 6;
const PORT_FREQUENCY: usize = 7;
const PORT_EXP_FM_GAIN: usize = 8;
const PORT_LIN_FM_GAIN: usize = 9;
const PORT_FEEDBACK: usize = 10;
const PORT_OUTPUT_MIX: usize = 11;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_MODULATION.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoOnly,
        parameters: vec![
            float_parameter(
                "input_gain",
                "Input Gain",
                None,
                Some(0.0),
                -40.0,
                10.0,
                0.1,
                ParameterUnit::Decibels,
            ),
            float_parameter(
                "sections",
                "Sections",
                None,
                Some(2.0),
                1.0,
                30.0,
                1.0,
                ParameterUnit::None,
            ),
            float_parameter(
                "frequency",
                "Frequency",
                None,
                Some(0.0),
                -6.0,
                6.0,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(0.0),
                -1.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "output_mix",
                "Output Mix",
                None,
                Some(0.0),
                -1.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
        ],
    }
}

fn build_mono_processor(
    sample_rate: f32,
    input_gain: f32,
    sections: f32,
    frequency: f32,
    feedback: f32,
    output_mix: f32,
) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    lv2::build_lv2_processor(
        &lib_path,
        PLUGIN_URI,
        sample_rate as f64,
        &bundle_path,
        &[PORT_AUDIO_IN],
        &[PORT_AUDIO_OUT],
        &[
            (PORT_FM_CV, 0.0),
            (PORT_EXP_FM_CV, 0.0),
            (PORT_LIN_FM_CV, 0.0),
            (PORT_INPUT_GAIN, input_gain),
            (PORT_SECTIONS, sections),
            (PORT_FREQUENCY, frequency),
            (PORT_EXP_FM_GAIN, 0.0),
            (PORT_LIN_FM_GAIN, 0.0),
            (PORT_FEEDBACK, feedback),
            (PORT_OUTPUT_MIX, output_mix),
        ],
    )
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let input_gain = required_f32(params, "input_gain").map_err(anyhow::Error::msg)?;
    let sections = required_f32(params, "sections").map_err(anyhow::Error::msg)?;
    let frequency = required_f32(params, "frequency").map_err(anyhow::Error::msg)?;
    let feedback = required_f32(params, "feedback").map_err(anyhow::Error::msg)?;
    let output_mix = required_f32(params, "output_mix").map_err(anyhow::Error::msg)?;

    let _ = layout;
    let processor = build_mono_processor(sample_rate, input_gain, sections, frequency, feedback, output_mix)?;
    Ok(BlockProcessor::Mono(Box::new(processor)))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: ModModelDefinition = ModModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: ModBackendKind::Lv2,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
