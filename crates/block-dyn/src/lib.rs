//! Dynamics implementations.
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub fn compressor_supported_models() -> &'static [&'static str] {
    registry::COMPRESSOR_SUPPORTED_MODELS
}

pub fn gate_supported_models() -> &'static [&'static str] {
    registry::GATE_SUPPORTED_MODELS
}

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

pub fn gate_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_gate_model_definition(model)?.schema)()
}

pub fn build_gate_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_gate_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_gate_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_gate_model_definition(model)?.build)(params, sample_rate, layout)
}
