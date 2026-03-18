//! Noise gate implementations.
pub mod basic;

use anyhow::{bail, Result};
use basic::{build_processor, model_schema, supports_model};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, StageProcessor};

pub const DEFAULT_GATE_MODEL: &str = "gate_basic";

pub fn gate_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_model(model) {
        Ok(model_schema())
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
    if supports_model(model) {
        match layout {
            AudioChannelLayout::Mono => {
                Ok(StageProcessor::Mono(build_processor(params, sample_rate)?))
            }
            AudioChannelLayout::Stereo => bail!(
                "gate model '{}' is mono-only and cannot build native stereo processing",
                model
            ),
        }
    } else {
        bail!("unsupported gate model '{}'", model)
    }
}
