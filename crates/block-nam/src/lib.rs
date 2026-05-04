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
        knob_layout: def.knob_layout,
        thumbnail_path: None,
        available: true,
    })
}

pub fn nam_display_name(model: &str) -> &'static str {
    registry::find_model_definition(model).map(|d| d.display_name).unwrap_or("")
}

pub fn nam_brand(model: &str) -> &'static str {
    registry::find_model_definition(model).map(|d| d.brand).unwrap_or("")
}

pub fn nam_type_label(model: &str) -> &'static str {
    nam_model_visual(model).map(|v| v.type_label).unwrap_or("")
}

pub fn nam_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn build_nam_processor(model: &str, params: &ParameterSet, sample_rate: f32) -> Result<BlockProcessor> {
    build_nam_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_nam_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}
