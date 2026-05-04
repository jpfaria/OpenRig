use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_benson_preamp";
pub const DISPLAY_NAME: &str = "Benson Preamp";
const BRAND: &str = "benson";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: EQ profile × drive (volume fixed at 6).
// 3 EQ profiles × 3 drive positions = 9 captures, full grid.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (eq, drive, file)
    ("flat",   "3", "pedals/benson_preamp/benson_preamp_01_t6b6d3v6.nam"),
    ("flat",   "6", "pedals/benson_preamp/benson_preamp_02_t6b6d6v6.nam"),
    ("flat",   "9", "pedals/benson_preamp/benson_preamp_03_t6b6d9v6.nam"),
    ("bright", "3", "pedals/benson_preamp/benson_preamp_04_t9b3d3v6.nam"),
    ("bright", "6", "pedals/benson_preamp/benson_preamp_05_t9b3d6v6.nam"),
    ("bright", "9", "pedals/benson_preamp/benson_preamp_06_t9b3d9v6.nam"),
    ("dark",   "3", "pedals/benson_preamp/benson_preamp_07_t4b8d3v6.nam"),
    ("dark",   "6", "pedals/benson_preamp/benson_preamp_08_t4b8d6v6.nam"),
    ("dark",   "9", "pedals/benson_preamp/benson_preamp_09_t4b8d9v6.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "eq",
            "EQ Profile",
            Some("Pedal"),
            Some("flat"),
            &[
                ("flat",   "Flat (T6 B6)"),
                ("bright", "Bright (T9 B3)"),
                ("dark",   "Dark (T4 B8)"),
            ],
        ),
        enum_parameter(
            "drive",
            "Drive",
            Some("Pedal"),
            Some("6"),
            &[
                ("3", "3"),
                ("6", "6"),
                ("9", "9"),
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
    let eq = required_string(params, "eq").map_err(anyhow::Error::msg)?;
    let drive = required_string(params, "drive").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(e, d, _)| *e == eq && *d == drive)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "gain '{}' has no capture for eq={} drive={}",
                MODEL_ID, eq, drive
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
