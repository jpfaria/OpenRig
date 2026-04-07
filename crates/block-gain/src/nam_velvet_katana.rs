use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "velvet_katana";
pub const DISPLAY_NAME: &str = "Velvet Katana";
const BRAND: &str = "velvet";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct VelvetKatanaCapture {
    character: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[VelvetKatanaCapture] = &[
    VelvetKatanaCapture { character: "country",     model_path: "pedals/velvet_katana/katana_country.nam" },
    VelvetKatanaCapture { character: "blues_bright", model_path: "pedals/velvet_katana/katana_blues_bright.nam" },
    VelvetKatanaCapture { character: "larry",       model_path: "pedals/velvet_katana/katana_larry.nam" },
    VelvetKatanaCapture { character: "brad",        model_path: "pedals/velvet_katana/katana_brad.nam" },
    VelvetKatanaCapture { character: "drive",       model_path: "pedals/velvet_katana/katana_drive.nam" },
    VelvetKatanaCapture { character: "drive_plus",  model_path: "pedals/velvet_katana/katana_drive_plus.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "character",
        "Character",
        Some("Pedal"),
        Some("larry"),
        &[
            ("country",      "Country"),
            ("blues_bright", "Blues Bright"),
            ("larry",        "Larry Carlton"),
            ("brad",         "Brad"),
            ("drive",        "Drive"),
            ("drive_plus",   "Drive ++"),
        ],
    )];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let capture = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(capture.model_path)?,
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
    let capture = resolve_capture(params)?;
    Ok(format!("model='{}'", capture.model_path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static VelvetKatanaCapture> {
    let character = required_string(params, "character").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.character == character)
        .ok_or_else(|| anyhow!("gain model '{}' does not support character='{}'", MODEL_ID, character))
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
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
