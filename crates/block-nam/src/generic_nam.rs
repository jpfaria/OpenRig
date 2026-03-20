use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::registry::NamModelDefinition;

const MODEL_ID: &str = nam::GENERIC_NAM_MODEL_ID;

fn schema() -> Result<ModelParameterSchema> {
    Ok(nam::model_schema_for(
        "nam",
        MODEL_ID,
        "Neural Amp Modeler",
        true,
    ))
}

fn build(params: &ParameterSet, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    nam::build_processor_for_layout(params, layout)
}

pub const MODEL_DEFINITION: NamModelDefinition = NamModelDefinition {
    id: MODEL_ID,
    schema,
    build,
};
