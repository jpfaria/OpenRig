use anyhow::{anyhow, Result};
use crate::registry::FullRigModelDefinition;
use crate::FullRigBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "vox_ac30";
pub const DISPLAY_NAME: &str = "AC30";
const BRAND: &str = "vox";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURE_STANDARD: &str = "full_rigs/vox_ac30/vox_ac30_cab.nam";
const CAPTURE_CLEAN_65PRINCE: &str = "full_rigs/vox_ac30/vox_ac30_clean_65prince.nam";

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("full_rig", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "character",
        "Character",
        Some("Amp"),
        Some("standard"),
        &[
            ("standard", "Standard"),
            ("clean_65prince", "Clean 65Prince"),
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
    let character = required_string(params, "character").map_err(anyhow::Error::msg)?;
    match character.as_str() {
        "standard" => Ok(CAPTURE_STANDARD),
        "clean_65prince" => Ok(CAPTURE_CLEAN_65PRINCE),
        other => Err(anyhow!(
            "full-rig model '{}' does not support character='{}'",
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
    supported_instruments: block_core::GUITAR_ACOUSTIC_BASS,
    knob_layout: &[],
};
