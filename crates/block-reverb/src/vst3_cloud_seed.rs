use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;
use anyhow::Result;
use block_core::param::{float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, EFFECT_TYPE_REVERB};
use std::sync::OnceLock;

pub const MODEL_ID: &str = "vst3_cloud_seed";
pub const DISPLAY_NAME: &str = "Cloud Seed";
const BRAND: &str = "cloud-seed";

/// Bundle directory name as installed by the Cloud Seed VST3 installer.
const BUNDLE_NAME: &str = "CloudSeed.vst3";

/// Cloud Seed parameter IDs (from the plugin source — Valdemar Erlingsson).
/// https://github.com/ValdemarOrn/CloudSeed
const PARAM_DECAY: u32 = 8;   // Line feedback / decay
const PARAM_WET: u32 = 19;    // Wet1 output level (0.0 = -inf dB, 1.0 = 0 dB)
const PARAM_DRY: u32 = 18;    // Dry output level

/// Cached plugin UID discovered from the installed bundle.
///
/// Scanning the bundle is done once per process lifetime and then reused for
/// every subsequent block instantiation.
static PLUGIN_UID: OnceLock<[u8; 16]> = OnceLock::new();

fn get_plugin_uid(bundle_path: &std::path::Path, sample_rate: f64) -> Result<[u8; 16]> {
    if let Some(uid) = PLUGIN_UID.get() {
        return Ok(*uid);
    }
    let infos = vst3_host::scan_vst3_bundle(bundle_path, sample_rate)?;
    let first = infos
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Cloud Seed: no audio plugin classes found in bundle"))?;
    let uid = first.uid;
    let _ = PLUGIN_UID.set(uid); // ignore race — same value
    Ok(uid)
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: EFFECT_TYPE_REVERB.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            float_parameter(
                "decay",
                "Decay",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let decay = required_f32(params, "decay").map_err(anyhow::Error::msg)? / 100.0;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)? / 100.0;

    let bundle_path = vst3_host::resolve_vst3_bundle(BUNDLE_NAME)?;
    let uid = get_plugin_uid(&bundle_path, sample_rate as f64)?;

    // Dry level: full signal when mix = 0, silence when mix = 1.
    // Wet level: silence when mix = 0, full signal when mix = 1.
    let dry_norm = 1.0 - mix as f64;
    let wet_norm = mix as f64;

    let vst3_params: &[(u32, f64)] = &[
        (PARAM_DECAY, decay as f64),
        (PARAM_DRY, dry_norm),
        (PARAM_WET, wet_norm),
    ];

    vst3_host::build_vst3_processor(&bundle_path, &uid, sample_rate as f64, layout, vst3_params)
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: ReverbModelDefinition = ReverbModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: ReverbBackendKind::Vst3,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
