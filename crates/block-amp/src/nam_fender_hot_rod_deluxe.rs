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

const CAPTURES: &[(&str, &str, &str)] = &[
    ("15_hot_rod_deluxe_warm_lead", "15 Hot Rod Deluxe - Warm Lead", "amps/fender_hot_rod_deluxe/15_hot_rod_deluxe_warm_lead.nam"),
    ("15_hot_rod_deluxe_vintage_sweet_", "15 Hot Rod Deluxe - Vintage Sweet Spot", "amps/fender_hot_rod_deluxe/15_hot_rod_deluxe_vintage_sweet_spot.nam"),
    ("15_hot_rod_deluxe_modern_overdri", "15 Hot Rod Deluxe - Modern Overdrive", "amps/fender_hot_rod_deluxe/15_hot_rod_deluxe_modern_overdrive.nam"),
    ("15_hot_rod_deluxe_womanly", "15 Hot Rod Deluxe - Womanly", "amps/fender_hot_rod_deluxe/15_hot_rod_deluxe_womanly.nam"),
    ("15_hot_rod_deluxe_vintage_overdr", "15 Hot Rod Deluxe - Vintage Overdrive", "amps/fender_hot_rod_deluxe/15_hot_rod_deluxe_vintage_overdrive.nam"),
    ("15_hot_rod_deluxe_bright_sweet_s", "15 Hot Rod Deluxe - Bright Sweet Spot", "amps/fender_hot_rod_deluxe/15_hot_rod_deluxe_bright_sweet_spot.nam"),
    ("15_hot_rod_deluxe_bright_clean", "15 Hot Rod Deluxe - Bright Clean", "amps/fender_hot_rod_deluxe/15_hot_rod_deluxe_bright_clean.nam"),
    ("15_hot_rod_deluxe_southern_snap", "15 Hot Rod Deluxe - Southern Snap", "amps/fender_hot_rod_deluxe/15_hot_rod_deluxe_southern_snap.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("15_hot_rod_deluxe_warm_lead"),
        &[
            ("15_hot_rod_deluxe_warm_lead", "15 Hot Rod Deluxe - Warm Lead"),
            ("15_hot_rod_deluxe_vintage_sweet_", "15 Hot Rod Deluxe - Vintage Sweet Spot"),
            ("15_hot_rod_deluxe_modern_overdri", "15 Hot Rod Deluxe - Modern Overdrive"),
            ("15_hot_rod_deluxe_womanly", "15 Hot Rod Deluxe - Womanly"),
            ("15_hot_rod_deluxe_vintage_overdr", "15 Hot Rod Deluxe - Vintage Overdrive"),
            ("15_hot_rod_deluxe_bright_sweet_s", "15 Hot Rod Deluxe - Bright Sweet Spot"),
            ("15_hot_rod_deluxe_bright_clean", "15 Hot Rod Deluxe - Bright Clean"),
            ("15_hot_rod_deluxe_southern_snap", "15 Hot Rod Deluxe - Southern Snap"),
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
    let key = required_string(params, "capture").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("amp '{}' has no capture '{}'", MODEL_ID, key))
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
