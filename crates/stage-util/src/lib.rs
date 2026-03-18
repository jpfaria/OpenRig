//! Utility implementations.
pub mod tuner_chromatic;

use anyhow::{bail, Result};
use tuner_chromatic::{model_schema, reference_hz_from_set, supports_model, ChromaticTuner};
use stage_core::param::{ModelParameterSchema, ParameterSet};

pub const DEFAULT_TUNER_MODEL: &str = "tuner_chromatic";

pub fn tuner_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_model(model) {
        Ok(model_schema())
    } else {
        bail!("unsupported tuner model '{}'", model)
    }
}

pub fn build_tuner_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: usize,
) -> Result<ChromaticTuner> {
    if !supports_model(model) {
        bail!("unsupported tuner model '{}'", model);
    }
    let _reference_hz = reference_hz_from_set(params)?;
    let (mut tuner, _handle) = ChromaticTuner::new(sample_rate);
    tuner.set_enabled(true);
    Ok(tuner)
}
