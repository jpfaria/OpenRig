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

// Two-axis pack: boost × switch.
// All captures are Clean Jazz channel. Boost = pre_boost vs normal.
// Switch encodes the front-panel Bright/MidBoost toggles.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (boost, switch, file)
    ("normal",    "neither",       "amps/ceriatone_ots_mini_20/ceriatone_ots_mini_20_clean_jazz_normal.nam"),
    ("pre_boost", "neither",       "amps/ceriatone_ots_mini_20/ceriatone_ots_mini_20_clean_jazz_pre_boost.nam"),
    ("pre_boost", "bright",        "amps/ceriatone_ots_mini_20/ceriatone_ots_mini_20_clean_jazz_pre_boost_bright.nam"),
    ("pre_boost", "midboost",      "amps/ceriatone_ots_mini_20/ceriatone_ots_mini_20_clean_jazz_pre_boost_midboost.nam"),
    ("pre_boost", "bright_mid",    "amps/ceriatone_ots_mini_20/ceriatone_ots_mini_20_clean_jazz_pre_boost_bright_midboost.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "boost",
            "Boost",
            Some("Amp"),
            Some("pre_boost"),
            &[
                ("normal",    "Normal"),
                ("pre_boost", "Pre-Boost"),
            ],
        ),
        enum_parameter(
            "switch",
            "Switch",
            Some("Amp"),
            Some("neither"),
            &[
                ("neither",    "Neither"),
                ("bright",     "Bright"),
                ("midboost",   "MidBoost"),
                ("bright_mid", "Bright + Mid"),
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
    let boost = required_string(params, "boost").map_err(anyhow::Error::msg)?;
    let switch = required_string(params, "switch").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(b, s, _)| *b == boost && *s == switch)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for boost={} switch={}",
                MODEL_ID, boost, switch
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
