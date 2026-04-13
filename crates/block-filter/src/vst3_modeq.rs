use crate::registry::FilterModelDefinition;
use crate::FilterBackendKind;
use anyhow::Result;
use block_core::param::{float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, EFFECT_TYPE_FILTER};
use std::sync::OnceLock;

pub const MODEL_ID: &str = "vst3_modeq";
pub const DISPLAY_NAME: &str = "modEQ";
const BRAND: &str = "modeq";
const BUNDLE_NAME: &str = "modEQ.vst3";

/// modEQ parameter IDs (Tobias Hienzsch, MIT).
/// https://github.com/tobanteAudio/modEQ
/// Band 0: low shelf, Band 1: peak, Band 2: high shelf
const PARAM_LOW_GAIN: u32 = 1;
const PARAM_MID_GAIN: u32 = 5;
const PARAM_HIGH_GAIN: u32 = 9;

static PLUGIN_UID: OnceLock<[u8; 16]> = OnceLock::new();

fn get_plugin_uid(bundle_path: &std::path::Path, sample_rate: f64) -> Result<[u8; 16]> {
    if let Some(uid) = PLUGIN_UID.get() {
        return Ok(*uid);
    }
    let infos = vst3_host::scan_vst3_bundle(bundle_path, sample_rate)?;
    let first = infos
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("modEQ: no audio plugin classes found in bundle"))?;
    let uid = first.uid;
    let _ = PLUGIN_UID.set(uid);
    Ok(uid)
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: EFFECT_TYPE_FILTER.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            // gain range: -24 to +24 dB — normalized as (gain + 24) / 48
            float_parameter("low", "Low", None, Some(0.0), -24.0, 24.0, 0.5, ParameterUnit::Decibels),
            float_parameter("mid", "Mid", None, Some(0.0), -24.0, 24.0, 0.5, ParameterUnit::Decibels),
            float_parameter("high", "High", None, Some(0.0), -24.0, 24.0, 0.5, ParameterUnit::Decibels),
        ],
    }
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let low = required_f32(params, "low").map_err(anyhow::Error::msg)?;
    let mid = required_f32(params, "mid").map_err(anyhow::Error::msg)?;
    let high = required_f32(params, "high").map_err(anyhow::Error::msg)?;

    let low_norm = ((low + 24.0) / 48.0).clamp(0.0, 1.0);
    let mid_norm = ((mid + 24.0) / 48.0).clamp(0.0, 1.0);
    let high_norm = ((high + 24.0) / 48.0).clamp(0.0, 1.0);

    let bundle_path = vst3_host::resolve_vst3_bundle(BUNDLE_NAME)?;
    let uid = get_plugin_uid(&bundle_path, sample_rate as f64)?;

    let vst3_params: &[(u32, f64)] = &[
        (PARAM_LOW_GAIN, low_norm as f64),
        (PARAM_MID_GAIN, mid_norm as f64),
        (PARAM_HIGH_GAIN, high_norm as f64),
    ];

    vst3_host::build_vst3_processor(&bundle_path, &uid, sample_rate as f64, layout, vst3_params)
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: FilterModelDefinition = FilterModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: FilterBackendKind::Vst3,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
