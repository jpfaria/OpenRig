//! Gain blocks such as boost, overdrive, distortion, and fuzz.
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum GainBackendKind {
    Native,
    Nam,
    Ir,
    Lv2,
    Vst3,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn gain_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            GainBackendKind::Native => "NATIVE",
            GainBackendKind::Nam => "NAM",
            GainBackendKind::Ir => "IR",
            GainBackendKind::Lv2 => "LV2",
            GainBackendKind::Vst3 => "VST3",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
        thumbnail_path: gain_thumbnail(model_id),
        available: registry::is_model_available(model_id),
    })
}

pub fn gain_display_name(model: &str) -> &'static str {
    registry::find_model_definition(model)
        .map(|d| d.display_name)
        .unwrap_or("")
}

pub fn gain_brand(model: &str) -> &'static str {
    registry::find_model_definition(model)
        .map(|d| d.brand)
        .unwrap_or("")
}

pub fn gain_type_label(model: &str) -> &'static str {
    gain_model_visual(model).map(|v| v.type_label).unwrap_or("")
}

pub fn gain_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn gain_asset_summary(model: &str, params: &ParameterSet) -> Result<String> {
    (registry::find_model_definition(model)?.asset_summary)(params)
}

pub fn validate_gain_params(model: &str, params: &ParameterSet) -> Result<()> {
    (registry::find_model_definition(model)?.validate)(params)
}

pub fn build_gain_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_gain_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_gain_processor_for_layout(
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
    anyhow::bail!("unsupported gain model '{}'", model)
}

/// Push every native model into the unified plugin-loader registry.
/// Called by `adapter-gui` at startup before plugin discovery freezes
/// the catalog.
pub fn register_natives() {
    registry::register_natives();
}

pub fn is_gain_model_available(model: &str) -> bool {
    registry::is_model_available(model)
}

/// Returns the catalog thumbnail path (relative to project root) for a model.
pub fn gain_thumbnail(model: &str) -> Option<&'static str> {
    registry::THUMBNAILS
        .iter()
        .find(|(id, _)| *id == model)
        .map(|(_, path)| *path)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
