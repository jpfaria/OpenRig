use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "demonfx_be_od";
pub const DISPLAY_NAME: &str = "BE-OD Clone";
const BRAND: &str = "demonfx";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct BeOdCapture {
    gain: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[BeOdCapture] = &[
    BeOdCapture { gain: "50",        model_path: "pedals/demonfx_be_od/be_od_g50.nam" },
    BeOdCapture { gain: "75",        model_path: "pedals/demonfx_be_od/be_od_g75.nam" },
    BeOdCapture { gain: "100",       model_path: "pedals/demonfx_be_od/be_od_g100.nam" },
    BeOdCapture { gain: "100_tight", model_path: "pedals/demonfx_be_od/be_od_g100_tight.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "gain",
        "Gain",
        Some("Pedal"),
        Some("75"),
        &[
            ("50",        "Low"),
            ("75",        "Medium"),
            ("100",       "High"),
            ("100_tight", "High Tight"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static BeOdCapture> {
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
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
