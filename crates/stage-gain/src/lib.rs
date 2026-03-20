//! Gain blocks such as boost, overdrive, distortion, and fuzz.
pub mod blues_overdrive_bd_2;

use anyhow::{bail, Result};
use blues_overdrive_bd_2::{
    asset_summary as blues_driver_asset_summary,
    build_processor_for_model as build_blues_driver_processor,
    model_schema as blues_driver_model_schema, supports_model as supports_blues_driver_model,
    validate_params as validate_blues_driver_params,
};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, StageProcessor};

pub fn supported_models() -> &'static [&'static str] {
    &[blues_overdrive_bd_2::MODEL_ID]
}

pub fn drive_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_blues_driver_model(model) {
        Ok(blues_driver_model_schema())
    } else {
        bail!("unsupported drive model '{}'", model)
    }
}

pub fn drive_asset_summary(model: &str, params: &ParameterSet) -> Result<String> {
    if supports_blues_driver_model(model) {
        blues_driver_asset_summary(params)
    } else {
        bail!("unsupported drive model '{}'", model)
    }
}

pub fn validate_drive_params(model: &str, params: &ParameterSet) -> Result<()> {
    if supports_blues_driver_model(model) {
        validate_blues_driver_params(params)
    } else {
        bail!("unsupported drive model '{}'", model)
    }
}

pub fn build_drive_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<StageProcessor> {
    build_drive_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_drive_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    _sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    if supports_blues_driver_model(model) {
        build_blues_driver_processor(params, layout)
    } else {
        bail!("unsupported drive model '{}'", model)
    }
}
