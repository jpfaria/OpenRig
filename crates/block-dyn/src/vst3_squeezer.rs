use crate::registry::DynModelDefinition;
use crate::DynBackendKind;
use anyhow::Result;
use block_core::param::{float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, EFFECT_TYPE_DYNAMICS};
use std::sync::OnceLock;

pub const MODEL_ID: &str = "vst3_squeezer";
pub const DISPLAY_NAME: &str = "Squeezer";
const BRAND: &str = "squeezer";
const BUNDLE_NAME: &str = "Squeezer.vst3";

/// Squeezer parameter IDs (mzuther, GPL-3.0-or-later).
/// https://github.com/mzuther/Squeezer
const PARAM_THRESHOLD: u32 = 0;
const PARAM_RATIO: u32 = 1;
const PARAM_MIX: u32 = 8;

static PLUGIN_UID: OnceLock<[u8; 16]> = OnceLock::new();

fn get_plugin_uid(bundle_path: &std::path::Path, sample_rate: f64) -> Result<[u8; 16]> {
    if let Some(uid) = PLUGIN_UID.get() {
        return Ok(*uid);
    }
    let infos = vst3_host::scan_vst3_bundle(bundle_path, sample_rate)?;
    let first = infos
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Squeezer: no audio plugin classes found in bundle"))?;
    let uid = first.uid;
    let _ = PLUGIN_UID.set(uid);
    Ok(uid)
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: EFFECT_TYPE_DYNAMICS.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            // threshold: -60 to 0 dB — normalized as (threshold + 60) / 60
            float_parameter("threshold", "Threshold", None, Some(-18.0), -60.0, 0.0, 0.5, ParameterUnit::Decibels),
            // ratio: 1:1 to 32:1 — normalized as (ratio - 1) / 31
            float_parameter("ratio", "Ratio", None, Some(4.0), 1.0, 32.0, 0.5, ParameterUnit::Ratio),
            float_parameter("mix", "Mix", None, Some(100.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let threshold = required_f32(params, "threshold").map_err(anyhow::Error::msg)?;
    let ratio = required_f32(params, "ratio").map_err(anyhow::Error::msg)?;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)? / 100.0;

    let threshold_norm = ((threshold + 60.0) / 60.0).clamp(0.0, 1.0);
    let ratio_norm = ((ratio - 1.0) / 31.0).clamp(0.0, 1.0);

    let bundle_path = vst3_host::resolve_vst3_bundle(BUNDLE_NAME)?;
    let uid = get_plugin_uid(&bundle_path, sample_rate as f64)?;

    let vst3_params: &[(u32, f64)] = &[
        (PARAM_THRESHOLD, threshold_norm as f64),
        (PARAM_RATIO, ratio_norm as f64),
        (PARAM_MIX, mix as f64),
    ];

    vst3_host::build_vst3_processor(&bundle_path, &uid, sample_rate as f64, layout, vst3_params)
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: DynModelDefinition = DynModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: DynBackendKind::Vst3,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
