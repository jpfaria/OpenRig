use anyhow::{anyhow, Result};
use crate::registry::FullRigModelDefinition;
use crate::FullRigBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "synergy_drect_mesa";
pub const DISPLAY_NAME: &str = "DRECT Mesa";
const BRAND: &str = "synergy";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURE_UNBOOSTED: &str = "full_rigs/synergy_drect_mesa/synergy_drect_unboosted.nam";
const CAPTURE_OD808_SM57: &str = "full_rigs/synergy_drect_mesa/synergy_drect_od808_sm57.nam";
const CAPTURE_SD1_SM58: &str = "full_rigs/synergy_drect_mesa/synergy_drect_sd1_sm58.nam";

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("full_rig", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "boost",
        "Boost",
        Some("Amp"),
        Some("unboosted"),
        &[
            ("unboosted", "Unboosted"),
            ("od808", "OD808"),
            ("sd1", "SD-1"),
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
    let boost = required_string(params, "boost").map_err(anyhow::Error::msg)?;
    match boost.as_str() {
        "unboosted" => Ok(CAPTURE_UNBOOSTED),
        "od808" => Ok(CAPTURE_OD808_SM57),
        "sd1" => Ok(CAPTURE_SD1_SM58),
        other => Err(anyhow!(
            "full-rig model '{}' does not support boost='{}'",
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
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};
