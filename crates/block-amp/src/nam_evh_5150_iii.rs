use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_evh_5150_iii";
pub const DISPLAY_NAME: &str = "5150 III";
const BRAND: &str = "evh";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: channel × boost. Full 2×4 cartesian, captured by OB1.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (channel, boost, file)
    ("blue", "fortin_33",       "amps/evh_5150_iii/ob1_evh_blue_fortin_33_full_rig.nam"),
    ("blue", "tc_805",          "amps/evh_5150_iii/ob1_evh_blue_805_full_rig.nam"),
    ("blue", "tc_spark",        "amps/evh_5150_iii/ob1_evh_blue_tc_spark_full_rig.nam"),
    ("blue", "precision_drive", "amps/evh_5150_iii/ob1_evh_blue_precision_drive_full_rig.nam"),
    ("red",  "fortin_33",       "amps/evh_5150_iii/ob1_evh_red_fortin_33_full_rig.nam"),
    ("red",  "tc_805",          "amps/evh_5150_iii/ob1_evh_red_805_full_rig.nam"),
    ("red",  "tc_spark",        "amps/evh_5150_iii/ob1_evh_red_tc_spark_full_rig.nam"),
    ("red",  "precision_drive", "amps/evh_5150_iii/ob1_evh_red_precision_drive_full_rig.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "channel",
            "Channel",
            Some("Amp"),
            Some("blue"),
            &[
                ("blue", "Blue (Crunch)"),
                ("red",  "Red (Lead)"),
            ],
        ),
        enum_parameter(
            "boost",
            "Boost",
            Some("Amp"),
            Some("fortin_33"),
            &[
                ("fortin_33",       "Fortin 33"),
                ("tc_805",          "TC 805"),
                ("tc_spark",        "TC Spark"),
                ("precision_drive", "Precision Drive"),
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
    let channel = required_string(params, "channel").map_err(anyhow::Error::msg)?;
    let boost = required_string(params, "boost").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(c, b, _)| *c == channel && *b == boost)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for channel={} boost={}",
                MODEL_ID, channel, boost
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
