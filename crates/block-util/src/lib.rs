//! Utility implementations.
mod processor;
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};

pub use processor::TunerProcessor;

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn tuner_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn build_tuner_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: usize,
) -> Result<Box<dyn TunerProcessor>> {
    (registry::find_model_definition(model)?.build)(params, sample_rate)
}
