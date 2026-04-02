use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_caps_phaser2";
pub const DISPLAY_NAME: &str = "CAPS Phaser II";
const BRAND: &str = "caps";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/caps/PhaserII";
const PLUGIN_DIR: &str = "mod-caps-PhaserII";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "PhaserII.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "PhaserII.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "PhaserII.dll";

// LV2 port indices (from TTL)
const PORT_RATE: usize = 0;
const PORT_LFO: usize = 1;
const PORT_DEPTH: usize = 2;
const PORT_SPREAD: usize = 3;
const PORT_RESONANCE: usize = 4;
const PORT_AUDIO_IN: usize = 5;
const PORT_AUDIO_OUT: usize = 6;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_MODULATION.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter(
                "rate",
                "Rate",
                None,
                Some(0.25),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "depth",
                "Depth",
                None,
                Some(0.75),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "spread",
                "Spread",
                None,
                Some(0.75),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "resonance",
                "Resonance",
                None,
                Some(0.25),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
        ],
    }
}

fn build_mono_processor(
    sample_rate: f32,
    rate: f32,
    depth: f32,
    spread: f32,
    resonance: f32,
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
            (PORT_RATE, rate),
            (PORT_LFO, 0.0), // Fixed to Sine
            (PORT_DEPTH, depth),
            (PORT_SPREAD, spread),
            (PORT_RESONANCE, resonance),
        ],
    )
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let rate = required_f32(params, "rate").map_err(anyhow::Error::msg)?;
    let depth = required_f32(params, "depth").map_err(anyhow::Error::msg)?;
    let spread = required_f32(params, "spread").map_err(anyhow::Error::msg)?;
    let resonance = required_f32(params, "resonance").map_err(anyhow::Error::msg)?;

    // Single mono instance — MonoToStereo duplicates output to both channels.
    // CAPS plugins use global state and crash (SIGSEGV) with multiple instances.
    let _ = layout;
    let processor = build_mono_processor(sample_rate, rate, depth, spread, resonance)?;
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
