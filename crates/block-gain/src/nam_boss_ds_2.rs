use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_boss_ds_2";
pub const DISPLAY_NAME: &str = "Boss DS-2";
const BRAND: &str = "boss";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: mode × character.
// 2 modes (Mode I: Distortion / Mode II: Turbo) × 3 character points
// (boost / default / max distortion) = 6 captures, full grid.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (mode, character, file)
    ("1", "boost",    "pedals/boss_ds_2/ds2_1_boost.nam"),
    ("1", "default",  "pedals/boss_ds_2/ds2_1_default.nam"),
    ("1", "max_dist", "pedals/boss_ds_2/ds2_1_max_dist.nam"),
    ("2", "boost",    "pedals/boss_ds_2/ds2_2_boost.nam"),
    ("2", "default",  "pedals/boss_ds_2/ds2_2_turbo_default.nam"),
    ("2", "max_dist", "pedals/boss_ds_2/ds2_2_turbo_max_dist.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "mode",
            "Mode",
            Some("Pedal"),
            Some("1"),
            &[
                ("1", "Mode I (Distortion)"),
                ("2", "Mode II (Turbo)"),
            ],
        ),
        enum_parameter(
            "character",
            "Character",
            Some("Pedal"),
            Some("default"),
            &[
                ("boost",    "Boost"),
                ("default",  "Default"),
                ("max_dist", "Max Distortion"),
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
    let mode = required_string(params, "mode").map_err(anyhow::Error::msg)?;
    let character = required_string(params, "character").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(m, c, _)| *m == mode && *c == character)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "gain '{}' has no capture for mode={} character={}",
                MODEL_ID, mode, character
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
