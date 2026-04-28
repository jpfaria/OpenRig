use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_ampeg_svt";
pub const DISPLAY_NAME: &str = "SVT";
const BRAND: &str = "ampeg";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: gain stage × mic.
// "Ultra" controls the SVT's high/low frequency boost circuit.
// Holes (e.g. "off" with sm75) return Err so the UI flags them.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (ultra, mic, file)
    ("hi",        "md_421", "amps/ampeg_svt/ampeg_svt_ultra_hi_md_421.nam"),
    ("hi",        "sm57",   "amps/ampeg_svt/ampeg_svt_ultra_hi_sm57.nam"),
    ("lo",        "md_421", "amps/ampeg_svt/ampeg_svt_ultra_lo_md_421.nam"),
    ("lo",        "sm57",   "amps/ampeg_svt/ampeg_svt_ultra_lo_sm57.nam"),
    ("off",       "md_421", "amps/ampeg_svt/ampeg_svt_md_421.nam"),
    ("off",       "sm75",   "amps/ampeg_svt/ampeg_svt_sm75.nam"),
    ("hi_lo_g10", "md_421", "amps/ampeg_svt/ampeg_svt_gain_10_ultra_lo_and_hi_md_421.nam"),
    ("hi_lo_g10", "sm57",   "amps/ampeg_svt/ampeg_svt_gain_10_ultra_lo_and_hi_sm57.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "ultra",
            "Ultra Switch",
            Some("Amp"),
            Some("hi"),
            &[
                ("off",       "Off"),
                ("hi",        "Hi"),
                ("lo",        "Lo"),
                ("hi_lo_g10", "Hi + Lo (Gain 10)"),
            ],
        ),
        enum_parameter(
            "mic",
            "Mic",
            Some("Amp"),
            Some("md_421"),
            &[
                ("md_421", "MD 421"),
                ("sm57",   "SM57"),
                ("sm75",   "SM75"),
            ],
        ),
    ];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let path = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let ultra = required_string(params, "ultra").map_err(anyhow::Error::msg)?;
    let mic = required_string(params, "mic").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(u, m, _)| *u == ultra && *m == mic)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for ultra={} mic={}",
                MODEL_ID, ultra, mic
            )
        })
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: AmpModelDefinition = AmpModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: AmpBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let path = resolve_capture(params)?;
    Ok(format!("model='{}'", path))
}
