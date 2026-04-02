// @platform: macos
use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_harmless";
pub const DISPLAY_NAME: &str = "Harmless";
const BRAND: &str = "shiro";

const PLUGIN_URI: &str = "https://github.com/ninodewit/SHIRO-Plugins/plugins/harmless";
const PLUGIN_DIR: &str = "Harmless";
const PLUGIN_BINARY: &str = "Harmless_dsp.dylib";

// LV2 port indices (from Harmless_dsp.ttl)
const PORT_AUDIO_IN_L: usize = 0;
const PORT_AUDIO_IN_R: usize = 1;
const PORT_AUDIO_OUT_L: usize = 2;
const PORT_AUDIO_OUT_R: usize = 3;
const PORT_RATE: usize = 4;
const PORT_SHAPE: usize = 5;
const PORT_TONE: usize = 6;
const PORT_PHASE: usize = 7;
const PORT_DEPTH: usize = 8;

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_MODULATION.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter("rate_hz", "Rate", None, Some(4.0), 0.1, 20.0, 0.1, ParameterUnit::Hertz),
            float_parameter("depth", "Depth", None, Some(100.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("shape", "Shape", None, Some(50.0), 1.0, 99.0, 1.0, ParameterUnit::Percent),
        ],
    })
}

fn build(params: &ParameterSet, sample_rate: f32, _layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let rate_hz = required_f32(params, "rate_hz").map_err(anyhow::Error::msg)?;
    let depth = required_f32(params, "depth").map_err(anyhow::Error::msg)?;
    let shape = required_f32(params, "shape").map_err(anyhow::Error::msg)? / 100.0;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    let processor = lv2::build_stereo_lv2_processor(
        &lib_path,
        PLUGIN_URI,
        sample_rate as f64,
        &bundle_path,
        &[PORT_AUDIO_IN_L, PORT_AUDIO_IN_R],
        &[PORT_AUDIO_OUT_L, PORT_AUDIO_OUT_R],
        &[
            (PORT_RATE, rate_hz),
            (PORT_SHAPE, shape),
            (PORT_TONE, 6000.0),
            (PORT_PHASE, 0.0),
            (PORT_DEPTH, depth),
        ],
    )?;
    Ok(BlockProcessor::Stereo(Box::new(processor)))
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
