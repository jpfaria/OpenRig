//! Filter implementations.
pub mod model_visual;
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum FilterBackendKind {
    Native,
    Nam,
    Ir,
    Lv2,
    Vst3,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn filter_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            FilterBackendKind::Native => "NATIVE",
            FilterBackendKind::Nam => "NAM",
            FilterBackendKind::Ir => "IR",
            FilterBackendKind::Lv2 => "LV2",
            FilterBackendKind::Vst3 => "VST3",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
    })
}

pub fn filter_display_name(model: &str) -> &'static str {
    registry::find_model_definition(model)
        .map(|d| d.display_name)
        .unwrap_or("")
}

pub fn filter_brand(model: &str) -> &'static str {
    registry::find_model_definition(model)
        .map(|d| d.brand)
        .unwrap_or("")
}

pub fn filter_type_label(model: &str) -> &'static str {
    filter_model_visual(model)
        .map(|v| v.type_label)
        .unwrap_or("")
}

pub fn filter_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn build_filter_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_filter_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_filter_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    if let Ok(definition) = registry::find_model_definition(model) {
        return (definition.build)(params, sample_rate, layout);
    }
    if let Some(package) = plugin_loader::registry::find(model) {
        return package.build_processor(params, sample_rate, layout);
    }
    anyhow::bail!("unsupported filter model '{}'", model)
}

/// Push every native model into the unified plugin-loader registry.
/// Called by `adapter-gui` at startup before plugin discovery freezes
/// the catalog.
pub fn register_natives() {
    registry::register_natives();
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
