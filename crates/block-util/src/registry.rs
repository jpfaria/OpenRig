use anyhow::{anyhow, Result};
use block_core::param::ModelParameterSchema;
use block_core::param::ParameterSet;
use crate::processor::TunerProcessor;

#[derive(Clone, Copy)]
pub struct UtilModelDefinition {
    pub id: &'static str,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub build: fn(&ParameterSet, usize) -> Result<Box<dyn TunerProcessor>>,
}

include!(concat!(env!("OUT_DIR"), "/generated_registry.rs"));

pub fn find_model_definition(model: &str) -> Result<&'static UtilModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported tuner model '{}'", model))
}
