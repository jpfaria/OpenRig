use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_marshall_6100_30th_anniversary";
pub const DISPLAY_NAME: &str = "6100 - 30th anniversary";
const BRAND: &str = "marshall";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: gain × boost. All Channel 2 Crunch A.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (gain, boost, file)
    ("low",  "boss_sd1",  "amps/marshall_6100_30th_anniversary/marshall_6100_channel2_crunch_a_low_gain_boss_sd1.nam"),
    ("mid",  "isocecles", "amps/marshall_6100_30th_anniversary/marshall_6100_channel2_crunch_a_mid_gain_isocecles.nam"),
    ("high", "isocecles", "amps/marshall_6100_30th_anniversary/marshall_6100_channel2_crunch_a_high_gain_isocecles.nam"),
    ("high", "boss_sd1",  "amps/marshall_6100_30th_anniversary/marshall_6100_channel2_crunch_a_high_gain_boss_sd1.nam"),
    ("high", "none",      "amps/marshall_6100_30th_anniversary/marshall_6100_channel2_crunch_a_high_gain_no_boost.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "gain",
            "Gain",
            Some("Amp"),
            Some("high"),
            &[
                ("low",  "Low"),
                ("mid",  "Mid"),
                ("high", "High"),
            ],
        ),
        enum_parameter(
            "boost",
            "Boost",
            Some("Amp"),
            Some("isocecles"),
            &[
                ("none",      "None"),
                ("isocecles", "Isocecles"),
                ("boss_sd1",  "Boss SD-1"),
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
    let gain = required_string(params, "gain").map_err(anyhow::Error::msg)?;
    let boost = required_string(params, "boost").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(g, b, _)| *g == gain && *b == boost)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for gain={} boost={}",
                MODEL_ID, gain, boost
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
