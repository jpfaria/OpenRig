use anyhow::{anyhow, Result};
use crate::registry::FullRigModelDefinition;
use crate::FullRigBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "dover_da50_mesa";
pub const DISPLAY_NAME: &str = "DA-50 + Mesa 4\u{00d7}12";
const BRAND: &str = "dover";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct DoverCapture {
    boost: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[DoverCapture] = &[
    DoverCapture { boost: "clean",   model_path: "full_rigs/dover_da50_mesa/da50_mesa_clean.nam" },
    DoverCapture { boost: "boosted", model_path: "full_rigs/dover_da50_mesa/da50_mesa_boosted.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("full_rig", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "boost",
        "Boost",
        Some("Rig"),
        Some("clean"),
        &[
            ("clean",   "Clean"),
            ("boosted", "Boosted"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static DoverCapture> {
    let boost = required_string(params, "boost").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.boost == boost)
        .ok_or_else(|| anyhow!("full-rig model '{}' does not support boost='{}'", MODEL_ID, boost))
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
