use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "rr_golden_clone";
pub const DISPLAY_NAME: &str = "Golden Clone";
const BRAND: &str = "rr";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct GoldenCloneCapture {
    setting: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[GoldenCloneCapture] = &[
    GoldenCloneCapture { setting: "5_4", model_path: "pedals/rr_golden_clone/golden_clone_54.nam" },
    GoldenCloneCapture { setting: "6_6", model_path: "pedals/rr_golden_clone/golden_clone_66.nam" },
    GoldenCloneCapture { setting: "2_7", model_path: "pedals/rr_golden_clone/golden_clone_27.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "setting",
        "Setting",
        Some("Pedal"),
        Some("6_6"),
        &[
            ("5_4", "5/4"),
            ("6_6", "6/6"),
            ("2_7", "2/7"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static GoldenCloneCapture> {
    let setting = required_string(params, "setting").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.setting == setting)
        .ok_or_else(|| anyhow!("gain model '{}' does not support setting='{}'", MODEL_ID, setting))
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
