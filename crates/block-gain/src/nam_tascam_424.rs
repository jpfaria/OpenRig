use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "tascam_424";
pub const DISPLAY_NAME: &str = "Tascam 424 Preamp";
const BRAND: &str = "tascam";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct Tascam424Capture {
    gain: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[Tascam424Capture] = &[
    Tascam424Capture { gain: "7",   model_path: "pedals/tascam_424/tascam_g7.nam" },
    Tascam424Capture { gain: "8",   model_path: "pedals/tascam_424/tascam_g8.nam" },
    Tascam424Capture { gain: "9",   model_path: "pedals/tascam_424/tascam_g9.nam" },
    Tascam424Capture { gain: "max", model_path: "pedals/tascam_424/tascam_gmax.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "gain",
        "Gain",
        Some("Pedal"),
        Some("8"),
        &[
            ("7",   "Low"),
            ("8",   "Medium"),
            ("9",   "High"),
            ("max", "Max"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static Tascam424Capture> {
    let gain = required_string(params, "gain").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.gain == gain)
        .ok_or_else(|| anyhow!("gain model '{}' does not support gain='{}'", MODEL_ID, gain))
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
