use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_browne_protein";
pub const DISPLAY_NAME: &str = "Browne Protein";
const BRAND: &str = "browne";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: channel × gain.
// 2 channels (Blue / Green) × 6 gain steps = 12 captures, full grid.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (channel, gain, file)
    ("blue",  "1", "pedals/browne_protein/blue_gain_1.nam"),
    ("blue",  "2", "pedals/browne_protein/blue_gain_2.nam"),
    ("blue",  "3", "pedals/browne_protein/blue_gain_3.nam"),
    ("blue",  "4", "pedals/browne_protein/blue_gain_4.nam"),
    ("blue",  "5", "pedals/browne_protein/blue_gain_5.nam"),
    ("blue",  "6", "pedals/browne_protein/blue_gain_6.nam"),
    ("green", "1", "pedals/browne_protein/green_gain_1.nam"),
    ("green", "2", "pedals/browne_protein/green_gain_2.nam"),
    ("green", "3", "pedals/browne_protein/green_gain_3.nam"),
    ("green", "4", "pedals/browne_protein/green_gain_4.nam"),
    ("green", "5", "pedals/browne_protein/green_gain_5.nam"),
    ("green", "6", "pedals/browne_protein/green_gain_6.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "channel",
            "Channel",
            Some("Pedal"),
            Some("blue"),
            &[
                ("blue",  "Blue (low gain)"),
                ("green", "Green (high gain)"),
            ],
        ),
        enum_parameter(
            "gain",
            "Gain",
            Some("Pedal"),
            Some("3"),
            &[
                ("1", "1"),
                ("2", "2"),
                ("3", "3"),
                ("4", "4"),
                ("5", "5"),
                ("6", "6"),
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

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let path = resolve_capture(params)?;
    Ok(format!("model='{}'", path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let channel = required_string(params, "channel").map_err(anyhow::Error::msg)?;
    let gain = required_string(params, "gain").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(c, g, _)| *c == channel && *g == gain)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "gain '{}' has no capture for channel={} gain={}",
                MODEL_ID, channel, gain
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
