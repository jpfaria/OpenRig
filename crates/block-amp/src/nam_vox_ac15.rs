use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_vox_ac15";
pub const DISPLAY_NAME: &str = "AC15";
const BRAND: &str = "vox";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: gain × channel. Full 4×2 cartesian on AC15CH.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (gain, channel, file)
    ("crystal_clean",    "normal", "amps/vox_ac15/vox_ac15ch_crystal_clean_normal.nam"),
    ("crystal_clean",    "tb",     "amps/vox_ac15/vox_ac15ch_crystal_clean_tb.nam"),
    ("edge_of_breakup",  "normal", "amps/vox_ac15/vox_ac15ch_edge_of_breakup_normal.nam"),
    ("edge_of_breakup",  "tb",     "amps/vox_ac15/vox_ac15ch_edge_of_breakup_tb.nam"),
    ("crunch",           "normal", "amps/vox_ac15/vox_ac15ch_crunch_normal.nam"),
    ("crunch",           "tb",     "amps/vox_ac15/vox_ac15ch_crunch_tb.nam"),
    ("overdriven",       "normal", "amps/vox_ac15/vox_ac15ch_overdriven_normal.nam"),
    ("overdriven",       "tb",     "amps/vox_ac15/vox_ac15ch_overdriven_tb.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "gain",
            "Gain",
            Some("Amp"),
            Some("edge_of_breakup"),
            &[
                ("crystal_clean",   "Crystal Clean"),
                ("edge_of_breakup", "Edge of Breakup"),
                ("crunch",          "Crunch"),
                ("overdriven",      "Overdriven"),
            ],
        ),
        enum_parameter(
            "channel",
            "Channel",
            Some("Amp"),
            Some("normal"),
            &[
                ("normal", "Normal"),
                ("tb",     "Top Boost"),
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
    let channel = required_string(params, "channel").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(g, c, _)| *g == gain && *c == channel)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for gain={} channel={}",
                MODEL_ID, gain, channel
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
