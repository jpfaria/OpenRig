//! Tremolo modulation implementations.
pub mod sine;

use anyhow::{bail, Result};
use sine::{build_processor, model_schema, supports_model};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::MonoProcessor;

pub const DEFAULT_TREMOLO_MODEL: &str = "sine_tremolo";

pub fn tremolo_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_model(model) {
        Ok(model_schema())
    } else {
        bail!("unsupported tremolo model '{}'", model)
    }
}

pub fn build_tremolo_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    if supports_model(model) {
        build_processor(params, sample_rate)
    } else {
        bail!("unsupported tremolo model '{}'", model)
    }
}
