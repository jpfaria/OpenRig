use anyhow::Result;
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "pot_boost";
pub const DISPLAY_NAME: &str = "PoT Boost";
const BRAND: &str = "pot";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURE_PATH: &str = "pedals/pot_boost/pot_boost.nam";

pub fn model_schema() -> ModelParameterSchema {
    model_schema_for("gain", MODEL_ID, DISPLAY_NAME, false)
}

pub fn build_processor_for_model(
    _params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(CAPTURE_PATH)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}

pub fn validate_params(_params: &ParameterSet) -> Result<()> {
    Ok(())
}

pub fn asset_summary(_params: &ParameterSet) -> Result<String> {
    Ok(format!("model='{}'", CAPTURE_PATH))
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
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
