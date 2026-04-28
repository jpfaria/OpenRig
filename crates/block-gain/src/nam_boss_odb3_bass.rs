use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_boss_odb3_bass";
pub const DISPLAY_NAME: &str = "Boss ODB3 Bass";
const BRAND: &str = "boss";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: blend × gain (EQ knobs fixed at v7.5/h5/l6).
// Sparse: 6 captures of a 3 × 3 grid. resolve_capture rejects holes
// so both knobs remain independent in the UI.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (blend, gain, file)
    ("25",  "5",   "pedals/boss_odb3_bass/boss_odb3_v7_5_h5_l6_blend_25_gain_5.nam"),
    ("25",  "7_5", "pedals/boss_odb3_bass/boss_odb3_v7_5_h5_l6_blend_25_gain_7_5.nam"),
    ("70",  "0",   "pedals/boss_odb3_bass/boss_odb3_v7_5_h5_l6_blend_70_gain_0.nam"),
    ("100", "0",   "pedals/boss_odb3_bass/boss_odb3_v7_5_h5_l6_blend_100_gain_0.nam"),
    ("100", "5",   "pedals/boss_odb3_bass/boss_odb3_v7_5_h5_l6_blend_100_gain_5.nam"),
    ("100", "7_5", "pedals/boss_odb3_bass/boss_odb3_v7_5_h5_l6_blend_100_gain_7_5.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "blend",
            "Blend",
            Some("Pedal"),
            Some("100"),
            &[
                ("25",  "25%"),
                ("70",  "70%"),
                ("100", "100% (wet)"),
            ],
        ),
        enum_parameter(
            "gain",
            "Gain",
            Some("Pedal"),
            Some("5"),
            &[
                ("0",   "0"),
                ("5",   "5"),
                ("7_5", "7.5"),
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
    let blend = required_string(params, "blend").map_err(anyhow::Error::msg)?;
    let gain = required_string(params, "gain").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(b, g, _)| *b == blend && *g == gain)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "gain '{}' has no capture for blend={} gain={}",
                MODEL_ID, blend, gain
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
