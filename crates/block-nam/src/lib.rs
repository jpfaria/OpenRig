mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub use nam::GENERIC_NAM_MODEL_ID;

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn nam_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn build_nam_processor(model: &str, params: &ParameterSet) -> Result<BlockProcessor> {
    build_nam_processor_for_layout(model, params, AudioChannelLayout::Mono)
}

pub fn build_nam_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, layout)
}
