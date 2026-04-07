use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "maxon_od808";
pub const DISPLAY_NAME: &str = "OD808 Overdrive";
const BRAND: &str = "maxon";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct Od808Capture {
    drive_percent: i32,
    model_path: &'static str,
}

const CAPTURES: &[Od808Capture] = &[
    Od808Capture { drive_percent: 0,   model_path: "pedals/maxon_od808/od808_drive0.nam" },
    Od808Capture { drive_percent: 25,  model_path: "pedals/maxon_od808/od808_drive25.nam" },
    Od808Capture { drive_percent: 50,  model_path: "pedals/maxon_od808/od808_drive50.nam" },
    Od808Capture { drive_percent: 75,  model_path: "pedals/maxon_od808/od808_drive75.nam" },
    Od808Capture { drive_percent: 100, model_path: "pedals/maxon_od808/od808_drive100.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![float_parameter(
        "drive_percent",
        "Drive",
        Some("Pedal"),
        Some(50.0),
        0.0,
        100.0,
        25.0,
        ParameterUnit::Percent,
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static Od808Capture> {
    let value = required_f32(params, "drive_percent").map_err(anyhow::Error::msg)?;
    let rounded = value.round();
    if (value - rounded).abs() > 1e-4 {
        return Err(anyhow!(
            "gain model '{}' requires 'drive_percent' to be a whole-number percentage, got {}",
            MODEL_ID, value
        ));
    }
    let drive = rounded as i32;
    CAPTURES
        .iter()
        .find(|c| c.drive_percent == drive)
        .ok_or_else(|| {
            anyhow!(
                "gain model '{}' does not support drive_percent={}",
                MODEL_ID, drive
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
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
