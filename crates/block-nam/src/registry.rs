use anyhow::{anyhow, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::GENERIC_NAM_MODEL_ID;

pub struct NamModelDefinition {
    pub id: &'static str,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub build: fn(&ParameterSet, AudioChannelLayout) -> Result<BlockProcessor>,
}

fn nam_schema() -> Result<ModelParameterSchema> {
    Ok(nam::model_schema_for(
        "nam",
        GENERIC_NAM_MODEL_ID,
        "Neural Amp Modeler",
        true,
    ))
}

fn nam_build(params: &ParameterSet, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    nam::build_processor_for_layout(params, layout)
}

const GENERIC_NAM: NamModelDefinition = NamModelDefinition {
    id: GENERIC_NAM_MODEL_ID,
    schema: nam_schema,
    build: nam_build,
};

pub const SUPPORTED_MODELS: &[&str] = &[GENERIC_NAM.id];

const MODEL_DEFINITIONS: &[NamModelDefinition] = &[GENERIC_NAM];

pub fn find_model_definition(model: &str) -> Result<&'static NamModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported nam model '{}'", model))
}
