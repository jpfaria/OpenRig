use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_prs_mt15";
pub const DISPLAY_NAME: &str = "MT15";
const BRAND: &str = "prs";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Single-axis: 7 voicings — Clean / Clean Breakup / Crunch / 4 Metal variants
// (7W power, TS-boost, preamp-only, sweet spot).
const CAPTURES: &[(&str, &str, &str)] = &[
    ("clean",            "Clean",             "amps/prs_mt15/prs_mt15_clean.nam"),
    ("clean_breakup",    "Clean Breakup",     "amps/prs_mt15/prs_mt_15_clean_break_up.nam"),
    ("crunch_red",       "Crunch (Red)",      "amps/prs_mt15/prs_mt_15_crunch_red.nam"),
    ("metal_7w",         "Metal 7W",          "amps/prs_mt15/prs_mt_15_metal_7_watts.nam"),
    ("metal_ts",         "Metal + TS",        "amps/prs_mt15/prs_mt_15_metal_ts.nam"),
    ("metal_pre",        "Metal Pre-Amp",     "amps/prs_mt15/prs_mt_15_metal_pre_amp.nam"),
    ("metal_sweet_spot", "Metal Sweet Spot",  "amps/prs_mt15/prs_mt_15_metal_sweet_spot.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "preset",
        "Preset",
        Some("Amp"),
        Some("clean"),
        &[
            ("clean",            "Clean"),
            ("clean_breakup",    "Clean Breakup"),
            ("crunch_red",       "Crunch (Red)"),
            ("metal_7w",         "Metal 7W"),
            ("metal_ts",         "Metal + TS"),
            ("metal_pre",        "Metal Pre-Amp"),
            ("metal_sweet_spot", "Metal Sweet Spot"),
        ],
    )];
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
    let key = required_string(params, "preset").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("amp '{}' has no preset '{}'", MODEL_ID, key))
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
