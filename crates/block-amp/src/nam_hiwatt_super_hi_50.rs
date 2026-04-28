use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_hiwatt_super_hi_50";
pub const DISPLAY_NAME: &str = "Super-Hi 50";
const BRAND: &str = "hiwatt";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("noon_04_blend_1", "[AMP] HWAT-SUPERHI50 Noon #04 - BLEND #1", "amps/hiwatt_super_hi_50/amp_hwat_superhi50_noon_04_blend_1_2.nam"),
    ("noon_04_blend_3", "[AMP] HWAT-SUPERHI50 Noon #04 - BLEND #3", "amps/hiwatt_super_hi_50/amp_hwat_superhi50_noon_04_blend_3_2.nam"),
    ("noon_04_di", "[AMP] HWAT-SUPERHI50 Noon #04 - DI", "amps/hiwatt_super_hi_50/amp_hwat_superhi50_noon_04_di_2.nam"),
    ("bright_overdrive_sm57", "[AMP] HWAT-SUPERHI50 Bright Overdrive+ - SM57", "amps/hiwatt_super_hi_50/amp_hwat_superhi50_bright_overdrive_sm57_2.nam"),
    ("bright_overdrive_di", "[AMP] HWAT-SUPERHI50 Bright Overdrive+ - DI", "amps/hiwatt_super_hi_50/amp_hwat_superhi50_bright_overdrive_di_2.nam"),
    ("bright_overdrive_blend_1", "[AMP] HWAT-SUPERHI50 Bright Overdrive+ - BLEND #1", "amps/hiwatt_super_hi_50/amp_hwat_superhi50_bright_overdrive_blend_1_2.nam"),
    ("bright_overdrive_blend_3", "[AMP] HWAT-SUPERHI50 Bright Overdrive+ - BLEND #3", "amps/hiwatt_super_hi_50/amp_hwat_superhi50_bright_overdrive_blend_3_2.nam"),
    ("bright_overdrive_blend_2", "[AMP] HWAT-SUPERHI50 Bright Overdrive - BLEND #2", "amps/hiwatt_super_hi_50/amp_hwat_superhi50_bright_overdrive_blend_2_2.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("noon_04_blend_1"),
        &[
            ("noon_04_blend_1", "[AMP] HWAT-SUPERHI50 Noon #04 - BLEND #1"),
            ("noon_04_blend_3", "[AMP] HWAT-SUPERHI50 Noon #04 - BLEND #3"),
            ("noon_04_di", "[AMP] HWAT-SUPERHI50 Noon #04 - DI"),
            ("bright_overdrive_sm57", "[AMP] HWAT-SUPERHI50 Bright Overdrive+ - SM57"),
            ("bright_overdrive_di", "[AMP] HWAT-SUPERHI50 Bright Overdrive+ - DI"),
            ("bright_overdrive_blend_1", "[AMP] HWAT-SUPERHI50 Bright Overdrive+ - BLEND #1"),
            ("bright_overdrive_blend_3", "[AMP] HWAT-SUPERHI50 Bright Overdrive+ - BLEND #3"),
            ("bright_overdrive_blend_2", "[AMP] HWAT-SUPERHI50 Bright Overdrive - BLEND #2"),
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
