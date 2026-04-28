use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_mxr_distortion";
pub const DISPLAY_NAME: &str = "MXR Distortion+";
const BRAND: &str = "mxr";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Single-axis: distortion knob position (output fixed at 3:00).
// 7 captures span the dial as clock positions.
const CAPTURES: &[(&str, &str)] = &[
    // (distortion, file)
    ("130",  "pedals/mxr_distortion/mxr_distortion_output300_dist130.nam"),
    ("300",  "pedals/mxr_distortion/mxr_distortion_output300_dist300.nam"),
    ("500",  "pedals/mxr_distortion/mxr_distortion_output300_dist500.nam"),
    ("700",  "pedals/mxr_distortion/mxr_distortion_output300_dist700.nam"),
    ("900",  "pedals/mxr_distortion/mxr_distortion_output300_dist900.nam"),
    ("1030", "pedals/mxr_distortion/mxr_distortion_output300_dist1030.nam"),
    ("1200", "pedals/mxr_distortion/mxr_distortion_output300_dist1200.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "distortion",
        "Distortion",
        Some("Pedal"),
        Some("700"),
        &[
            ("130",  "1:30"),
            ("300",  "3:00"),
            ("500",  "5:00"),
            ("700",  "7:00"),
            ("900",  "9:00"),
            ("1030", "10:30"),
            ("1200", "12:00"),
        ],
    )];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let path = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(path)?,
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
    let path = resolve_capture(params)?;
    Ok(format!("model='{}'", path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let distortion = required_string(params, "distortion").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(d, _)| *d == distortion)
        .map(|(_, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "gain '{}' has no capture for distortion={}",
                MODEL_ID, distortion
            )
        })
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
