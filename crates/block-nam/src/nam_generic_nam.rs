use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::registry::NamModelDefinition;
use crate::NamBlockBackendKind;

const MODEL_ID: &str = nam::GENERIC_NAM_MODEL_ID;
const DISPLAY_NAME: &str = "Neural Amp Modeler";

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
    display_name: DISPLAY_NAME,
    brand: "",
    backend_kind: NamBlockBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
};
