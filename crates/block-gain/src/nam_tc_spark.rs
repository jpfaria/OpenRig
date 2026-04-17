use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "tc_spark";
pub const DISPLAY_NAME: &str = "Spark";
const BRAND: &str = "tc-electronic";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURE_CLEAN: &str = "pedals/tc_spark/tc_spark_clean.nam";
const CAPTURE_MID: &str = "pedals/tc_spark/tc_spark_mid.nam";

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("gain", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "character",
        "Character",
        Some("Tone"),
        Some("clean"),
        &[("clean", "Clean"), ("mid", "Mid")],
    )];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let model_path = resolve_capture_path(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(model_path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture_path(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let path = resolve_capture_path(params)?;
    Ok(format!("model='{}'", path))
}

fn resolve_capture_path(params: &ParameterSet) -> Result<&'static str> {
    let character = required_string(params, "character").map_err(anyhow::Error::msg)?;
    match character.as_str() {
        "clean" => Ok(CAPTURE_CLEAN),
        "mid" => Ok(CAPTURE_MID),
        other => Err(anyhow!(
            "gain model '{}' does not support character='{}'",
            MODEL_ID,
            other
        )),
    }
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
