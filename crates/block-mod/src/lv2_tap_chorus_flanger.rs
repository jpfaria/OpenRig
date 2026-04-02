use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_tap_chorus_flanger";
pub const DISPLAY_NAME: &str = "TAP Chorus/Flanger";
const BRAND: &str = "tap";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/chorusflanger";
const PLUGIN_DIR: &str = "tap-chorusflanger";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "tap_chorusflanger.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "tap_chorusflanger.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "tap_chorusflanger.dll";

// LV2 port indices (from TTL)
const PORT_FREQUENCY: usize = 0;
const PORT_LR_PHASE_SHIFT: usize = 1;
const PORT_DEPTH: usize = 2;
const PORT_DELAY: usize = 3;
const PORT_CONTOUR: usize = 4;
const PORT_DRY_LEVEL: usize = 5;
const PORT_WET_LEVEL: usize = 6;
const PORT_AUDIO_IN_L: usize = 7;
const PORT_AUDIO_IN_R: usize = 8;
const PORT_AUDIO_OUT_L: usize = 9;
const PORT_AUDIO_OUT_R: usize = 10;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_MODULATION.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            float_parameter(
                "frequency",
                "Frequency",
                None,
                Some(1.75),
                0.0,
                5.0,
                0.01,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "lr_phase_shift",
                "L/R Phase Shift",
                None,
                Some(90.0),
                0.0,
                180.0,
                1.0,
                ParameterUnit::None,
            ),
            float_parameter(
                "depth",
                "Depth",
                None,
                Some(75.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "delay",
                "Delay",
                None,
                Some(25.0),
                0.0,
                100.0,
                0.1,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "contour",
                "Contour",
                None,
                Some(100.0),
                20.0,
                20000.0,
                1.0,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "dry_level",
                "Dry Level",
                None,
                Some(-3.0),
                -90.0,
                20.0,
                0.1,
                ParameterUnit::Decibels,
            ),
            float_parameter(
                "wet_level",
                "Wet Level",
                None,
                Some(-3.0),
                -90.0,
                20.0,
                0.1,
                ParameterUnit::Decibels,
            ),
        ],
    }
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    _layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let frequency = required_f32(params, "frequency").map_err(anyhow::Error::msg)?;
    let lr_phase_shift = required_f32(params, "lr_phase_shift").map_err(anyhow::Error::msg)?;
    let depth = required_f32(params, "depth").map_err(anyhow::Error::msg)?;
    let delay = required_f32(params, "delay").map_err(anyhow::Error::msg)?;
    let contour = required_f32(params, "contour").map_err(anyhow::Error::msg)?;
    let dry_level = required_f32(params, "dry_level").map_err(anyhow::Error::msg)?;
    let wet_level = required_f32(params, "wet_level").map_err(anyhow::Error::msg)?;

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
            (PORT_FREQUENCY, frequency),
            (PORT_LR_PHASE_SHIFT, lr_phase_shift),
            (PORT_DEPTH, depth),
            (PORT_DELAY, delay),
            (PORT_CONTOUR, contour),
            (PORT_DRY_LEVEL, dry_level),
            (PORT_WET_LEVEL, wet_level),
        ],
    )?;
    Ok(BlockProcessor::Stereo(Box::new(processor)))
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
