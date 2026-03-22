mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum NamBlockBackendKind {
    Native,
    Nam,
    Ir,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn nam_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            NamBlockBackendKind::Native => "NATIVE",
            NamBlockBackendKind::Nam => "NAM",
            NamBlockBackendKind::Ir => "IR",
        },
        supported_instruments: def.supported_instruments,
    })
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
