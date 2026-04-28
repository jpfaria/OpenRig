use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_boss_hm_3";
pub const DISPLAY_NAME: &str = "Boss HM-3";
const BRAND: &str = "boss";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Three-axis pack: lows × highs × dist.
// 2 × 2 × 2 = 8 captures, full grid.
const CAPTURES: &[(&str, &str, &str, &str)] = &[
    // (lows, highs, dist, file)
    ("5",  "5",  "5",  "pedals/boss_hm_3/boss_hm3_lows5_highs5_dist5_boss_hm3.nam"),
    ("5",  "5",  "10", "pedals/boss_hm_3/boss_hm3_lows5_highs5_dist10_boss_hm3.nam"),
    ("5",  "10", "5",  "pedals/boss_hm_3/boss_hm3_lows5_highs10_dist5_boss_hm3.nam"),
    ("5",  "10", "10", "pedals/boss_hm_3/boss_hm3_lows5_highs10_dist10_boss_hm3.nam"),
    ("10", "5",  "5",  "pedals/boss_hm_3/boss_hm3_lows10_highs5_dist5_boss_hm3.nam"),
    ("10", "5",  "10", "pedals/boss_hm_3/boss_hm3_lows10_highs5_dist10_boss_hm3.nam"),
    ("10", "10", "5",  "pedals/boss_hm_3/boss_hm3_lows10_highs10_dist5_boss_hm3.nam"),
    ("10", "10", "10", "pedals/boss_hm_3/boss_hm3_lows10_highs10_dist10_boss_hm3.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "lows",
            "Lows",
            Some("Pedal"),
            Some("5"),
            &[
                ("5",  "5"),
                ("10", "10"),
            ],
        ),
        enum_parameter(
            "highs",
            "Highs",
            Some("Pedal"),
            Some("5"),
            &[
                ("5",  "5"),
                ("10", "10"),
            ],
        ),
        enum_parameter(
            "dist",
            "Distortion",
            Some("Pedal"),
            Some("5"),
            &[
                ("5",  "5"),
                ("10", "10"),
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
    let lows = required_string(params, "lows").map_err(anyhow::Error::msg)?;
    let highs = required_string(params, "highs").map_err(anyhow::Error::msg)?;
    let dist = required_string(params, "dist").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(l, h, d, _)| *l == lows && *h == highs && *d == dist)
        .map(|(_, _, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "gain '{}' has no capture for lows={} highs={} dist={}",
                MODEL_ID, lows, highs, dist
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
