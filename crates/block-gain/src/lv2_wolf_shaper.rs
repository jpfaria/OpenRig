use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_wolf_shaper";
pub const DISPLAY_NAME: &str = "Wolf Shaper";
const BRAND: &str = "wolf";

const PLUGIN_URI: &str = "https://github.com/pdesaulniers/wolf-shaper";
const PLUGIN_DIR: &str = "wolf-shaper";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "wolf-shaper_dsp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "wolf-shaper_dsp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "wolf-shaper_dsp.dll";

// LV2 port indices (from TTL)
const PORT_AUDIO_IN_L: usize = 0;
const PORT_AUDIO_IN_R: usize = 1;
const PORT_AUDIO_OUT_L: usize = 2;
const PORT_AUDIO_OUT_R: usize = 3;
const PORT_ATOM_IN: usize = 4;
const PORT_ATOM_OUT: usize = 5;
const PORT_PREGAIN: usize = 6;
const PORT_WET: usize = 7;
const PORT_POSTGAIN: usize = 8;
const PORT_REMOVEDC: usize = 9;
const PORT_OVERSAMPLE: usize = 10;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter(
                "pregain",
                "Pre Gain",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "wet",
                "Wet",
                None,
                Some(100.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "postgain",
                "Post Gain",
                None,
                Some(100.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

fn validate_params(params: &ParameterSet) -> Result<()> {
    let _ = required_f32(params, "pregain").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "wet").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "postgain").map_err(anyhow::Error::msg)?;
    Ok(())
}

fn asset_summary(_params: &ParameterSet) -> Result<String> {
    Ok(format!("lv2='{}'", MODEL_ID))
}

fn build_mono_processor(
    sample_rate: f32,
    pregain: f32,
    wet: f32,
    postgain: f32,
) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    lv2::build_lv2_processor_full(
        &lib_path,
        PLUGIN_URI,
        sample_rate as f64,
        &bundle_path,
        &[PORT_AUDIO_IN_L],
        &[PORT_AUDIO_OUT_L],
        &[
            (PORT_PREGAIN, pregain),
            (PORT_WET, wet),
            (PORT_POSTGAIN, postgain),
            (PORT_REMOVEDC, 1.0),
            (PORT_OVERSAMPLE, 0.0),
        ],
        &[PORT_ATOM_IN, PORT_ATOM_OUT],
        &[PORT_AUDIO_IN_R, PORT_AUDIO_OUT_R],
    )
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    // PreGain: 0-100% maps to 0-2
    let pregain = required_f32(params, "pregain").map_err(anyhow::Error::msg)? / 100.0 * 2.0;
    let wet = required_f32(params, "wet").map_err(anyhow::Error::msg)? / 100.0;
    let postgain = required_f32(params, "postgain").map_err(anyhow::Error::msg)? / 100.0;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, pregain, wet, postgain)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
            let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
            let processor = lv2::build_stereo_lv2_processor_with_atoms(
                &lib_path,
                PLUGIN_URI,
                sample_rate as f64,
                &bundle_path,
                &[PORT_AUDIO_IN_L, PORT_AUDIO_IN_R],
                &[PORT_AUDIO_OUT_L, PORT_AUDIO_OUT_R],
                &[
                    (PORT_PREGAIN, pregain),
                    (PORT_WET, wet),
                    (PORT_POSTGAIN, postgain),
                    (PORT_REMOVEDC, 1.0),
                    (PORT_OVERSAMPLE, 0.0),
                ],
                &[PORT_ATOM_IN, PORT_ATOM_OUT],
            )?;
            Ok(BlockProcessor::Stereo(Box::new(processor)))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Lv2,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};
