//! Gain blocks such as boost, overdrive, distortion, and fuzz.
pub mod blues_overdrive_bd_2;
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn drive_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn drive_asset_summary(model: &str, params: &ParameterSet) -> Result<String> {
    (registry::find_model_definition(model)?.asset_summary)(params)
}

pub fn validate_drive_params(model: &str, params: &ParameterSet) -> Result<()> {
    (registry::find_model_definition(model)?.validate)(params)
}

pub fn build_drive_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_drive_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_drive_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    _sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, layout)
}
