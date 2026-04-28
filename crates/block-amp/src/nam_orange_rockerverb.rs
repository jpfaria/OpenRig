use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_orange_rockerverb";
pub const DISPLAY_NAME: &str = "Rockerverb";
const BRAND: &str = "orange";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: voicing × boost. All captures Canov+Arnold MK3.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (voicing, boost, file)
    ("clean",     "none",         "amps/orange_rockerverb/orange_rockerverb_mk3_clean_canov_arnold.nam"),
    ("crunch_g5", "none",         "amps/orange_rockerverb/orange_rockerverb_mk3_crunch_g5_canov_arnold.nam"),
    ("rock_g6",   "none",         "amps/orange_rockerverb/orange_rockerverb_mk3_rock_g6_canov_arnold.nam"),
    ("higain_g5", "fortin_ts808", "amps/orange_rockerverb/orange_rockerverb_mk3_higain_g5_fortin_ts808_canov_arnold.nam"),
    ("higain_g6", "ti_boost",     "amps/orange_rockerverb/orange_rockerverb_mk3_higain_g6_ti_boost_canov_arnold.nam"),
    ("higain_g7", "none",         "amps/orange_rockerverb/orange_rockerverb_mk3_higain_g7_canov_arnold.nam"),
    ("jim_root",  "none",         "amps/orange_rockerverb/orange_rockerverb_mk3_jim_root_canov_arnold.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "voicing",
            "Voicing",
            Some("Amp"),
            Some("higain_g7"),
            &[
                ("clean",     "Clean"),
                ("crunch_g5", "Crunch (G5)"),
                ("rock_g6",   "Rock (G6)"),
                ("higain_g5", "Hi-Gain (G5)"),
                ("higain_g6", "Hi-Gain (G6)"),
                ("higain_g7", "Hi-Gain (G7)"),
                ("jim_root",  "Jim Root"),
            ],
        ),
        enum_parameter(
            "boost",
            "Boost",
            Some("Amp"),
            Some("none"),
            &[
                ("none",         "None"),
                ("fortin_ts808", "Fortin TS808"),
                ("ti_boost",     "TI Boost"),
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
    let voicing = required_string(params, "voicing").map_err(anyhow::Error::msg)?;
    let boost = required_string(params, "boost").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(v, b, _)| *v == voicing && *b == boost)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for voicing={} boost={}",
                MODEL_ID, voicing, boost
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
