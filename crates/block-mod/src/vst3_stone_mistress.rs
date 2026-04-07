use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::Result;
use block_core::param::{float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, EFFECT_TYPE_MODULATION};
use std::sync::OnceLock;

pub const MODEL_ID: &str = "vst3_stone_mistress";
pub const DISPLAY_NAME: &str = "Stone Mistress";
const BRAND: &str = "stone-mistress";
const BUNDLE_NAME: &str = "StoneMistress.vst3";

/// Stone Mistress (flanger) parameter IDs.
/// Rate, depth, feedback, and mix are the core params.
const PARAM_RATE: u32 = 0;
const PARAM_DEPTH: u32 = 1;
const PARAM_FEEDBACK: u32 = 2;
const PARAM_MIX: u32 = 3;

static PLUGIN_UID: OnceLock<[u8; 16]> = OnceLock::new();

fn get_plugin_uid(bundle_path: &std::path::Path, sample_rate: f64) -> Result<[u8; 16]> {
    if let Some(uid) = PLUGIN_UID.get() {
        return Ok(*uid);
    }
    let infos = vst3_host::scan_vst3_bundle(bundle_path, sample_rate)?;
    let first = infos
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Stone Mistress: no audio plugin classes found in bundle"))?;
    let uid = first.uid;
    let _ = PLUGIN_UID.set(uid);
    Ok(uid)
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: EFFECT_TYPE_MODULATION.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            float_parameter("rate_hz", "Rate", None, Some(0.5), 0.1, 10.0, 0.1, ParameterUnit::Hertz),
            float_parameter("depth", "Depth", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("feedback", "Feedback", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("mix", "Mix", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let rate_hz = required_f32(params, "rate_hz").map_err(anyhow::Error::msg)?;
    let depth = required_f32(params, "depth").map_err(anyhow::Error::msg)? / 100.0;
    let feedback = required_f32(params, "feedback").map_err(anyhow::Error::msg)? / 100.0;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)? / 100.0;

    // Normalize rate from 0.1–10 Hz to 0.0–1.0
    let rate_norm = ((rate_hz - 0.1) / 9.9).clamp(0.0, 1.0);

    let bundle_path = vst3_host::resolve_vst3_bundle(BUNDLE_NAME)?;
    let uid = get_plugin_uid(&bundle_path, sample_rate as f64)?;

    let vst3_params: &[(u32, f64)] = &[
        (PARAM_RATE, rate_norm as f64),
        (PARAM_DEPTH, depth as f64),
        (PARAM_FEEDBACK, feedback as f64),
        (PARAM_MIX, mix as f64),
    ];

    vst3_host::build_vst3_processor(&bundle_path, &uid, sample_rate as f64, layout, vst3_params)
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: ModModelDefinition = ModModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: ModBackendKind::Vst3,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
