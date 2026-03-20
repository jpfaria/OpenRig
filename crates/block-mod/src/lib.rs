//! Modulation implementations.
pub mod tremolo_sine;
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn tremolo_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn build_tremolo_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_tremolo_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_tremolo_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}
