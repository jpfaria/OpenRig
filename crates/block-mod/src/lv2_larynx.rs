// @platform: macos
use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_larynx";
pub const DISPLAY_NAME: &str = "Larynx";
const BRAND: &str = "shiro";

const PLUGIN_URI: &str = "https://github.com/ninodewit/SHIRO-Plugins/plugins/larynx";
const PLUGIN_DIR: &str = "Larynx.lv2";
const PLUGIN_BINARY: &str = "Larynx_dsp.dylib";

// LV2 port indices (from Larynx_dsp.ttl)
const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;
const PORT_TONE: usize = 2;
const PORT_DEPTH: usize = 3;
const PORT_RATE: usize = 4;

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_MODULATION.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoOnly,
        parameters: vec![
            float_parameter("rate_hz", "Rate", None, Some(5.0), 0.1, 10.0, 0.1, ParameterUnit::Hertz),
            float_parameter("depth_ms", "Depth", None, Some(1.0), 0.1, 5.0, 0.1, ParameterUnit::Milliseconds),
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

fn build_mono(sample_rate: f32, rate_hz: f32, depth_ms: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = resolve_lib_path()?;
    let bundle_path = resolve_bundle_path()?;
    lv2::build_lv2_processor(
        &lib_path,
        PLUGIN_URI,
        sample_rate as f64,
        &bundle_path,
        &[PORT_AUDIO_IN],
        &[PORT_AUDIO_OUT],
        &[
            (PORT_TONE, 6000.0),
            (PORT_DEPTH, depth_ms),
            (PORT_RATE, rate_hz),
        ],
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let rate_hz = required_f32(params, "rate_hz").map_err(anyhow::Error::msg)?;
    let depth_ms = required_f32(params, "depth_ms").map_err(anyhow::Error::msg)?;

    let _ = layout;
    let processor = build_mono(sample_rate, rate_hz, depth_ms)?;
    Ok(BlockProcessor::Mono(Box::new(processor)))
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
