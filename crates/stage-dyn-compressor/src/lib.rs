//! Compressor implementations.
pub mod studio_clean;

use anyhow::{bail, Result};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::MonoProcessor;
use studio_clean::{build_processor, model_schema, supports_model};

pub const DEFAULT_COMPRESSOR_MODEL: &str = "studio_clean";

pub fn compressor_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_model(model) {
        Ok(model_schema())
    } else {
        bail!("unsupported compressor model '{}'", model)
    }
}

pub fn build_compressor_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    if supports_model(model) {
        build_processor(params, sample_rate)
    } else {
        bail!("unsupported compressor model '{}'", model)
    }
}
