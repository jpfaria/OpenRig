//! Modulation implementations.
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ModBackendKind {
    Native,
    Nam,
    Ir,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn mod_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            ModBackendKind::Native => "NATIVE",
            ModBackendKind::Nam => "NAM",
            ModBackendKind::Ir => "IR",
        },
        supported_instruments: def.supported_instruments,
    })
}

pub fn modulation_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn build_modulation_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_modulation_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_modulation_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}
