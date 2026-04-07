//! Utility implementations.
mod processor;
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData, StreamHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum UtilBackendKind {
    Native,
    Nam,
    Ir,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn util_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            UtilBackendKind::Native => "NATIVE",
            UtilBackendKind::Nam => "NAM",
            UtilBackendKind::Ir => "IR",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
    })
}

pub fn utility_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn util_stream_kind(model_id: &str) -> &'static str {
    registry::util_stream_kind(model_id)
}

pub fn build_utility_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: usize,
    layout: AudioChannelLayout,
) -> Result<(BlockProcessor, Option<StreamHandle>)> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}
