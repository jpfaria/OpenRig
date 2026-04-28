use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_ehx_soul_food";
pub const DISPLAY_NAME: &str = "EHX Soul Food";
const BRAND: &str = "ehx";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Single-axis: drive position (volume fixed at 10, treble at 5).
const CAPTURES: &[(&str, &str)] = &[
    // (drive, file)
    ("0",   "pedals/ehx_soul_food/gp_eh_soul_food_vol_10_treb_05_drive_00.nam"),
    ("25",  "pedals/ehx_soul_food/gp_eh_soul_food_vol_10_treb_05_drive_25.nam"),
    ("40",  "pedals/ehx_soul_food/gp_eh_soul_food_vol_10_treb_05_drive_40.nam"),
    ("50",  "pedals/ehx_soul_food/gp_eh_soul_food_vol_10_treb_05_drive_50.nam"),
    ("60",  "pedals/ehx_soul_food/gp_eh_soul_food_vol_10_treb_05_drive_60.nam"),
    ("75",  "pedals/ehx_soul_food/gp_eh_soul_food_vol_10_treb_05_drive_75.nam"),
    ("100", "pedals/ehx_soul_food/gp_eh_soul_food_vol_10_treb_05_drive_100.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "drive",
        "Drive",
        Some("Pedal"),
        Some("50"),
        &[
            ("0",   "0%"),
            ("25",  "25%"),
            ("40",  "40%"),
            ("50",  "50%"),
            ("60",  "60%"),
            ("75",  "75%"),
            ("100", "100%"),
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

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let path = resolve_capture(params)?;
    Ok(format!("model='{}'", path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let drive = required_string(params, "drive").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(d, _)| *d == drive)
        .map(|(_, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "gain '{}' has no capture for drive={}",
                MODEL_ID, drive
            )
        })
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};
