use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_fender_hot_rod_deluxe";
pub const DISPLAY_NAME: &str = "Hot Rod Deluxe";
const BRAND: &str = "fender";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Single-axis preset pack — 8 voicing presets without a natural cartesian split.
// Labels stripped of "15 Hot Rod Deluxe - " prefix.
const CAPTURES: &[(&str, &str, &str)] = &[
    ("bright_clean",       "Bright Clean",        "amps/fender_hot_rod_deluxe/15_hot_rod_deluxe_bright_clean.nam"),
    ("bright_sweet_spot",  "Bright Sweet Spot",   "amps/fender_hot_rod_deluxe/15_hot_rod_deluxe_bright_sweet_spot.nam"),
    ("vintage_sweet_spot", "Vintage Sweet Spot",  "amps/fender_hot_rod_deluxe/15_hot_rod_deluxe_vintage_sweet_spot.nam"),
    ("vintage_overdrive",  "Vintage Overdrive",   "amps/fender_hot_rod_deluxe/15_hot_rod_deluxe_vintage_overdrive.nam"),
    ("modern_overdrive",   "Modern Overdrive",    "amps/fender_hot_rod_deluxe/15_hot_rod_deluxe_modern_overdrive.nam"),
    ("warm_lead",          "Warm Lead",           "amps/fender_hot_rod_deluxe/15_hot_rod_deluxe_warm_lead.nam"),
    ("southern_snap",      "Southern Snap",       "amps/fender_hot_rod_deluxe/15_hot_rod_deluxe_southern_snap.nam"),
    ("womanly",            "Womanly",             "amps/fender_hot_rod_deluxe/15_hot_rod_deluxe_womanly.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "preset",
        "Preset",
        Some("Amp"),
        Some("bright_clean"),
        &[
            ("bright_clean",       "Bright Clean"),
            ("bright_sweet_spot",  "Bright Sweet Spot"),
            ("vintage_sweet_spot", "Vintage Sweet Spot"),
            ("vintage_overdrive",  "Vintage Overdrive"),
            ("modern_overdrive",   "Modern Overdrive"),
            ("warm_lead",          "Warm Lead"),
            ("southern_snap",      "Southern Snap"),
            ("womanly",            "Womanly"),
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
