use anyhow::{anyhow, Result};
use crate::registry::FullRigModelDefinition;
use crate::FullRigBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "fender_super_reverb_1977";
pub const DISPLAY_NAME: &str = "Super Reverb 1977";
const BRAND: &str = "fender";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURE_SM57: &str = "full_rigs/fender_super_reverb_1977/fender_super_reverb_sm57.nam";
const CAPTURE_AKG414: &str = "full_rigs/fender_super_reverb_1977/fender_super_reverb_akg414.nam";
const CAPTURE_SM57_AKG414: &str = "full_rigs/fender_super_reverb_1977/fender_super_reverb_sm57_akg414.nam";

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("full_rig", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "mic",
        "Mic",
        Some("Cab"),
        Some("sm57"),
        &[
            ("sm57", "SM57"),
            ("akg414", "AKG 414"),
            ("sm57_akg414", "SM57 + AKG 414"),
        ],
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
    let mic = required_string(params, "mic").map_err(anyhow::Error::msg)?;
    match mic.as_str() {
        "sm57" => Ok(CAPTURE_SM57),
        "akg414" => Ok(CAPTURE_AKG414),
        "sm57_akg414" => Ok(CAPTURE_SM57_AKG414),
        other => Err(anyhow!(
            "full-rig model '{}' does not support mic='{}'",
            MODEL_ID,
            other
        )),
    }
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
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
