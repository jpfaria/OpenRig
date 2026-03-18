//! Dynamics implementations.
pub mod compressor_studio_clean;
pub mod gate_basic;

use anyhow::{bail, Result};
use compressor_studio_clean::{
    build_processor as build_compressor, model_schema as compressor_schema,
    supports_model as supports_compressor_model,
};
use gate_basic::{
    build_processor as build_gate, model_schema as gate_schema,
    supports_model as supports_gate_model,
};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, StageProcessor};

pub fn compressor_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_compressor_model(model) {
        Ok(compressor_schema())
    } else {
        bail!("unsupported compressor model '{}'", model)
    }
}

pub fn build_compressor_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<StageProcessor> {
    build_compressor_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_compressor_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    if supports_compressor_model(model) {
        match layout {
            AudioChannelLayout::Mono => {
                Ok(StageProcessor::Mono(build_compressor(params, sample_rate)?))
            }
            AudioChannelLayout::Stereo => bail!(
                "compressor model '{}' is mono-only and cannot build native stereo processing",
                model
            ),
        }
    } else {
        bail!("unsupported compressor model '{}'", model)
    }
}

pub fn gate_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_gate_model(model) {
        Ok(gate_schema())
    } else {
        bail!("unsupported gate model '{}'", model)
    }
}

pub fn build_gate_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<StageProcessor> {
    build_gate_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_gate_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    if supports_gate_model(model) {
        match layout {
            AudioChannelLayout::Mono => Ok(StageProcessor::Mono(build_gate(params, sample_rate)?)),
            AudioChannelLayout::Stereo => bail!(
                "gate model '{}' is mono-only and cannot build native stereo processing",
                model
            ),
        }
    } else {
        bail!("unsupported gate model '{}'", model)
    }
}
