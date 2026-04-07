use crate::registry::DelayModelDefinition;
use crate::DelayBackendKind;
use anyhow::Result;
use block_core::param::{float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, EFFECT_TYPE_DELAY};
use std::sync::OnceLock;

pub const MODEL_ID: &str = "vst3_cocoa_delay";
pub const DISPLAY_NAME: &str = "Cocoa Delay";
const BRAND: &str = "klevgrand";
const BUNDLE_NAME: &str = "CocoacDelay.vst3";

/// Cocoa Delay parameter IDs (Klevgränd VST3).
/// Time and feedback are the core params; mix controls wet/dry.
const PARAM_TIME: u32 = 0;
const PARAM_FEEDBACK: u32 = 1;
const PARAM_MIX: u32 = 4;

static PLUGIN_UID: OnceLock<[u8; 16]> = OnceLock::new();

fn get_plugin_uid(bundle_path: &std::path::Path, sample_rate: f64) -> Result<[u8; 16]> {
    if let Some(uid) = PLUGIN_UID.get() {
        return Ok(*uid);
    }
    let infos = vst3_host::scan_vst3_bundle(bundle_path, sample_rate)?;
    let first = infos
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Cocoa Delay: no audio plugin classes found in bundle"))?;
    let uid = first.uid;
    let _ = PLUGIN_UID.set(uid);
    Ok(uid)
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: EFFECT_TYPE_DELAY.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            float_parameter("time_ms", "Time", None, Some(350.0), 1.0, 2000.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("feedback", "Feedback", None, Some(40.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("mix", "Mix", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let time_ms = required_f32(params, "time_ms").map_err(anyhow::Error::msg)?;
    let feedback = required_f32(params, "feedback").map_err(anyhow::Error::msg)? / 100.0;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)? / 100.0;

    // Normalize time to 0.0..=1.0 over 0–2000ms range
    let time_norm = (time_ms / 2000.0).clamp(0.0, 1.0);

    let bundle_path = vst3_host::resolve_vst3_bundle(BUNDLE_NAME)?;
    let uid = get_plugin_uid(&bundle_path, sample_rate as f64)?;

    let vst3_params: &[(u32, f64)] = &[
        (PARAM_TIME, time_norm as f64),
        (PARAM_FEEDBACK, feedback as f64),
        (PARAM_MIX, mix as f64),
    ];

    vst3_host::build_vst3_processor(&bundle_path, &uid, sample_rate as f64, layout, vst3_params)
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: DelayModelDefinition = DelayModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: DelayBackendKind::Vst3,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
