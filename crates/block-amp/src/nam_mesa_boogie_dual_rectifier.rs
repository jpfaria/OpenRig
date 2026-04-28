use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_mesa_boogie_dual_rectifier";
pub const DISPLAY_NAME: &str = "Dual Rectifier";
const BRAND: &str = "mesa";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Single-axis: 5 progressive rhythm-channel preset levels. #1 crunch through
// #4 increasing gain, #5 with TS808 boost on top.
const CAPTURES: &[(&str, &str, &str)] = &[
    ("crunch",      "Crunch",         "amps/mesa_boogie_dual_rectifier/1_mesa_dual_rectifier_2025_crunch_rhythm_1_2.nam"),
    ("rhythm_2",    "Rhythm 2",       "amps/mesa_boogie_dual_rectifier/2_mesa_dual_rectifier_2025_rhythm_2_2.nam"),
    ("rhythm_3",    "Rhythm 3",       "amps/mesa_boogie_dual_rectifier/3_mesa_dual_rectifier_2025_rhythm_3_2.nam"),
    ("rhythm_4",    "Rhythm 4",       "amps/mesa_boogie_dual_rectifier/4_mesa_dual_rectifier_2025_rhythm_4_2.nam"),
    ("ts808_boost", "TS808 Boost",    "amps/mesa_boogie_dual_rectifier/5_mesa_dual_rectifier_2025_ts808_boost_rhythm_5_2.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "preset",
        "Preset",
        Some("Amp"),
        Some("crunch"),
        &[
            ("crunch",      "Crunch"),
            ("rhythm_2",    "Rhythm 2"),
            ("rhythm_3",    "Rhythm 3"),
            ("rhythm_4",    "Rhythm 4"),
            ("ts808_boost", "TS808 Boost"),
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
    let key = required_string(params, "preset").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("amp '{}' has no preset '{}'", MODEL_ID, key))
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
