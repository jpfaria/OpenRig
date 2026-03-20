use anyhow::{anyhow, Result};
use block_core::param::ModelParameterSchema;
use block_core::param::ParameterSet;

use crate::tuner_chromatic::ChromaticTuner;
use crate::tuner_chromatic;

pub struct UtilModelDefinition {
    pub id: &'static str,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub build: fn(&ParameterSet, usize) -> Result<ChromaticTuner>,
}

fn tuner_schema() -> Result<ModelParameterSchema> {
    Ok(tuner_chromatic::model_schema())
}

fn tuner_build(params: &ParameterSet, sample_rate: usize) -> Result<ChromaticTuner> {
    let _reference_hz = tuner_chromatic::reference_hz_from_set(params)?;
    let (mut tuner, _handle) = ChromaticTuner::new(sample_rate);
    tuner.set_enabled(true);
    Ok(tuner)
}

const TUNER_CHROMATIC: UtilModelDefinition = UtilModelDefinition {
    id: tuner_chromatic::MODEL_ID,
    schema: tuner_schema,
    build: tuner_build,
};

pub const SUPPORTED_MODELS: &[&str] = &[TUNER_CHROMATIC.id];

const MODEL_DEFINITIONS: &[UtilModelDefinition] = &[TUNER_CHROMATIC];

pub fn find_model_definition(model: &str) -> Result<&'static UtilModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported tuner model '{}'", model))
}
