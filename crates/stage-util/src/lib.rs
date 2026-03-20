//! Utility implementations.
pub mod tuner_chromatic;

use anyhow::{bail, Result};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use tuner_chromatic::{model_schema, reference_hz_from_set, supports_model, ChromaticTuner};

pub fn supported_models() -> &'static [&'static str] {
    &[tuner_chromatic::MODEL_ID]
}

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
