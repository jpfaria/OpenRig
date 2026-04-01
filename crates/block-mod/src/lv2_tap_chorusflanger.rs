// @platform: macos
use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_tap_chorusflanger";
pub const DISPLAY_NAME: &str = "TAP Chorus/Flanger";
const BRAND: &str = "tap";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/chorusflanger";
const PLUGIN_DIR: &str = "tap-chorusflanger.lv2";
const PLUGIN_BINARY: &str = "tap_chorusflanger.dylib";

// LV2 port indices (from tap_chorusflanger.ttl)
// Controls: 0=Frequency, 1=LRPhaseShift, 2=Depth, 3=Delay, 4=Contour, 5=DryLevel, 6=WetLevel
// Audio: 7=InputL, 8=InputR, 9=OutputL, 10=OutputR
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

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_MODULATION.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            float_parameter("rate_hz", "Rate", None, Some(1.75), 0.0, 5.0, 0.01, ParameterUnit::Hertz),
            float_parameter("depth", "Depth", None, Some(75.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("delay_ms", "Delay", None, Some(25.0), 0.0, 100.0, 1.0, ParameterUnit::Milliseconds),
        ],
    })
}

fn resolve_lib_path() -> Result<String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let candidates = [
        exe_dir.as_ref().map(|d| d.join("../../").join(lv2::default_lv2_lib_dir()).join(PLUGIN_BINARY)),
        Some(std::path::PathBuf::from(lv2::default_lv2_lib_dir()).join(PLUGIN_BINARY)),
    ];
    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }
    anyhow::bail!("LV2 binary '{}' not found in '{}'", PLUGIN_BINARY, lv2::default_lv2_lib_dir())
}

fn resolve_bundle_path() -> Result<String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let candidates = [
        exe_dir.as_ref().map(|d| d.join("../../plugins").join(PLUGIN_DIR)),
        Some(std::path::PathBuf::from("plugins").join(PLUGIN_DIR)),
    ];
    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }
    anyhow::bail!("LV2 bundle '{}' not found in plugins/", PLUGIN_DIR)
}

fn build(params: &ParameterSet, sample_rate: f32, _layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let rate_hz = required_f32(params, "rate_hz").map_err(anyhow::Error::msg)?;
    let depth = required_f32(params, "depth").map_err(anyhow::Error::msg)?;
    let delay_ms = required_f32(params, "delay_ms").map_err(anyhow::Error::msg)?;

    let lib_path = resolve_lib_path()?;
    let bundle_path = resolve_bundle_path()?;

    let processor = lv2::build_stereo_lv2_processor(
        &lib_path,
        PLUGIN_URI,
        sample_rate as f64,
        &bundle_path,
        &[PORT_AUDIO_IN_L, PORT_AUDIO_IN_R],
        &[PORT_AUDIO_OUT_L, PORT_AUDIO_OUT_R],
        &[
            (PORT_FREQUENCY, rate_hz),
            (PORT_LR_PHASE_SHIFT, 90.0),
            (PORT_DEPTH, depth),
            (PORT_DELAY, delay_ms),
            (PORT_CONTOUR, 100.0),
            (PORT_DRY_LEVEL, -3.0),
            (PORT_WET_LEVEL, -3.0),
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
