use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_ceriatone_ots_mini_20";
pub const DISPLAY_NAME: &str = "OTS Mini 20";
const BRAND: &str = "ceriatone";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("ceriatone_ots_mini_20_clean_jazz", "Ceriatone OTS Mini 20 - Clean Jazz Pre-Boost Bright MidBoost", "amps/ceriatone_ots_mini_20/ceriatone_ots_mini_20_clean_jazz_pre_boost_bright_midboost.nam"),
    ("ceriatone_ots_mini_20_clean_jazz_325795", "Ceriatone OTS Mini 20 - Clean Jazz Pre-Boost MidBoost", "amps/ceriatone_ots_mini_20/ceriatone_ots_mini_20_clean_jazz_pre_boost_midboost.nam"),
    ("ceriatone_ots_mini_20_clean_jazz_325794", "Ceriatone OTS Mini 20 - Clean Jazz Pre-Boost", "amps/ceriatone_ots_mini_20/ceriatone_ots_mini_20_clean_jazz_pre_boost.nam"),
    ("ceriatone_ots_mini_20_clean_jazz_325799", "Ceriatone OTS Mini 20 - Clean Jazz Pre-Boost Bright", "amps/ceriatone_ots_mini_20/ceriatone_ots_mini_20_clean_jazz_pre_boost_bright.nam"),
    ("ceriatone_ots_mini_20_clean_jazz_325798", "Ceriatone OTS Mini 20 - Clean Jazz Normal", "amps/ceriatone_ots_mini_20/ceriatone_ots_mini_20_clean_jazz_normal.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("ceriatone_ots_mini_20_clean_jazz"),
        &[
            ("ceriatone_ots_mini_20_clean_jazz", "Ceriatone OTS Mini 20 - Clean Jazz Pre-Boost Bright MidBoost"),
            ("ceriatone_ots_mini_20_clean_jazz_325795", "Ceriatone OTS Mini 20 - Clean Jazz Pre-Boost MidBoost"),
            ("ceriatone_ots_mini_20_clean_jazz_325794", "Ceriatone OTS Mini 20 - Clean Jazz Pre-Boost"),
            ("ceriatone_ots_mini_20_clean_jazz_325799", "Ceriatone OTS Mini 20 - Clean Jazz Pre-Boost Bright"),
            ("ceriatone_ots_mini_20_clean_jazz_325798", "Ceriatone OTS Mini 20 - Clean Jazz Normal"),
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
