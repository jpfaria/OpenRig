use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "blues_overdrive_bd_2";
pub const DISPLAY_NAME: &str = "Blues Driver BD-2";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BluesOverdriveParams {
    pub gain_percent: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BluesOverdriveCapture {
    pub params: BluesOverdriveParams,
    pub model_path: &'static str,
}

pub const CAPTURES: &[BluesOverdriveCapture] = &[
    capture(
        25,
        "captures/nam/pedals/boss_blues_driver_bd_2/Boss Blues Driver BD-2 Gain 25percent.nam",
    ),
    capture(
        50,
        "captures/nam/pedals/boss_blues_driver_bd_2/Boss Blues Driver BD-2 Gain 50percent.nam",
    ),
    capture(
        75,
        "captures/nam/pedals/boss_blues_driver_bd_2/Boss Blues Driver BD-2 Gain 75percent.nam",
    ),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("gain", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![float_parameter(
        "gain_percent",
        "Gain",
        Some("Drive"),
        Some(50.0),
        25.0,
        75.0,
        25.0,
        ParameterUnit::Percent,
    )];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let capture = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        capture.model_path,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static BluesOverdriveCapture> {
    let requested = BluesOverdriveParams {
        gain_percent: read_percent(params, "gain_percent")?,
    };

    CAPTURES
        .iter()
        .find(|capture| capture.params == requested)
        .ok_or_else(|| {
            anyhow!(
                "gain model '{}' does not support gain_percent={}",
                MODEL_ID,
                requested.gain_percent
            )
        })
}

fn read_percent(params: &ParameterSet, path: &str) -> Result<i32> {
    let value = required_f32(params, path).map_err(anyhow::Error::msg)?;
    let rounded = value.round();
    if (value - rounded).abs() > 1e-4 {
        return Err(anyhow!(
            "gain model '{}' requires '{}' to be a whole-number percentage, got {}",
            MODEL_ID,
            path,
            value
        ));
    }
    Ok(rounded as i32)
}

const fn capture(gain_percent: i32, model_path: &'static str) -> BluesOverdriveCapture {
    BluesOverdriveCapture {
        params: BluesOverdriveParams { gain_percent },
        model_path,
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    _sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    build_processor_for_model(params, layout)
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "boss",
    backend_kind: GainBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
};
