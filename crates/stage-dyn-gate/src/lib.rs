//! Noise gate implementations.
pub mod basic;

use anyhow::{bail, Result};
use basic::{build_processor, model_schema, supports_model};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::MonoProcessor;

pub const DEFAULT_GATE_MODEL: &str = "noise_gate_basic";

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
) -> Result<Box<dyn MonoProcessor>> {
    if supports_model(model) {
        build_processor(params, sample_rate)
    } else {
        bail!("unsupported gate model '{}'", model)
    }
}
