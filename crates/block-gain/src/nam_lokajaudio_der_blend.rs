use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "lokajaudio_der_blend";
pub const DISPLAY_NAME: &str = "Der Blend";
const BRAND: &str = "lokajaudio";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct DerBlendCapture {
    character: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[DerBlendCapture] = &[
    DerBlendCapture { character: "off",        model_path: "pedals/lokajaudio_der_blend/der_blend_off.nam" },
    DerBlendCapture { character: "mid",        model_path: "pedals/lokajaudio_der_blend/der_blend_mid.nam" },
    DerBlendCapture { character: "high",       model_path: "pedals/lokajaudio_der_blend/der_blend_high.nam" },
    DerBlendCapture { character: "high_boost", model_path: "pedals/lokajaudio_der_blend/der_blend_high_boost.nam" },
    DerBlendCapture { character: "max",        model_path: "pedals/lokajaudio_der_blend/der_blend_max.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "character",
        "Character",
        Some("Pedal"),
        Some("high"),
        &[
            ("off",        "Off"),
            ("mid",        "Mid"),
            ("high",       "High"),
            ("high_boost", "High Boost"),
            ("max",        "Max"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static DerBlendCapture> {
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
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};
