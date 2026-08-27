//! Responsibility: serves the compressor family of this crate.

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::registry;

pub fn compressor_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_compressor_model_definition(model)?.schema)()
}

pub fn build_compressor_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_compressor_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_compressor_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_compressor_model_definition(model)?.build)(params, sample_rate, layout)
}
