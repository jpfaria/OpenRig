use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, EFFECT_TYPE_GAIN};
use std::sync::OnceLock;

pub const MODEL_ID: &str = "vst3_chow_tape";
pub const DISPLAY_NAME: &str = "CHOW Tape";
const BRAND: &str = "chow-dsp";
const BUNDLE_NAME: &str = "CHOWTapeModel.vst3";

/// CHOW Tape Model parameter IDs (jatinchowdhury18, BSD-3-Clause).
/// https://github.com/jatinchowdhury18/AnalogTapeModel
/// Drive, Saturation, and Mix are the core user-facing params.
const PARAM_DRIVE: u32 = 0;
const PARAM_SATURATION: u32 = 1;
const PARAM_MIX: u32 = 2;

static PLUGIN_UID: OnceLock<[u8; 16]> = OnceLock::new();

fn get_plugin_uid(bundle_path: &std::path::Path, sample_rate: f64) -> Result<[u8; 16]> {
    if let Some(uid) = PLUGIN_UID.get() {
        return Ok(*uid);
    }
    let infos = vst3_host::scan_vst3_bundle(bundle_path, sample_rate)?;
    let first = infos
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("CHOW Tape: no audio plugin classes found in bundle"))?;
    let uid = first.uid;
    let _ = PLUGIN_UID.set(uid);
    Ok(uid)
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            float_parameter("drive", "Drive", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("saturation", "Saturation", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("mix", "Mix", None, Some(100.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn validate_params(params: &ParameterSet) -> Result<()> {
    let _ = required_f32(params, "drive").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "saturation").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "mix").map_err(anyhow::Error::msg)?;
    Ok(())
}

fn asset_summary(_params: &ParameterSet) -> Result<String> {
    Ok(format!("vst3='{}'", MODEL_ID))
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let drive = required_f32(params, "drive").map_err(anyhow::Error::msg)? / 100.0;
    let saturation = required_f32(params, "saturation").map_err(anyhow::Error::msg)? / 100.0;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)? / 100.0;

    let bundle_path = vst3_host::resolve_vst3_bundle(BUNDLE_NAME)?;
    let uid = get_plugin_uid(&bundle_path, sample_rate as f64)?;

    let vst3_params: &[(u32, f64)] = &[
        (PARAM_DRIVE, drive as f64),
        (PARAM_SATURATION, saturation as f64),
        (PARAM_MIX, mix as f64),
    ];

    vst3_host::build_vst3_processor(&bundle_path, &uid, sample_rate as f64, layout, vst3_params)
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Vst3,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
