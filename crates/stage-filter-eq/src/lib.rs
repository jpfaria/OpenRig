//! Filter/EQ implementations.
pub mod three_band_basic;

use anyhow::{bail, Result};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::MonoProcessor;
use three_band_basic::{build_processor, model_schema, supports_model};

pub const DEFAULT_EQ_MODEL: &str = "three_band_basic";

pub fn eq_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_model(model) {
        Ok(model_schema())
    } else {
        bail!("unsupported eq model '{}'", model)
    }
}

pub fn build_eq_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    if supports_model(model) {
        build_processor(params, sample_rate)
    } else {
        bail!("unsupported eq model '{}'", model)
    }
}
