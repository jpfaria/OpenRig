use anyhow::Result;
use crate::registry::FullRigModelDefinition;
use crate::FullRigBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "marshall_super_100_1966";
pub const DISPLAY_NAME: &str = "Super 100 1966";
const BRAND: &str = "marshall";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURE_PATH: &str = "full_rigs/marshall_super_100_1966/marshall_sa100_i_edge_bal_cab.nam";

pub fn model_schema() -> ModelParameterSchema {
    model_schema_for("full_rig", MODEL_ID, DISPLAY_NAME, false)
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

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: FullRigModelDefinition = FullRigModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: FullRigBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};
