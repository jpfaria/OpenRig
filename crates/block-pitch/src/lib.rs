//! Pitch correction block implementations.
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PitchBackendKind {
    Native,
    Nam,
    Ir,
    Lv2,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn pitch_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            PitchBackendKind::Native => "NATIVE",
            PitchBackendKind::Nam => "NAM",
            PitchBackendKind::Ir => "IR",
            PitchBackendKind::Lv2 => "LV2",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
        thumbnail_path: pitch_thumbnail(model_id),
        available: registry::is_model_available(model_id),
    })
}

pub fn pitch_display_name(model: &str) -> &'static str {
    registry::find_model_definition(model)
        .map(|d| d.display_name)
        .unwrap_or("")
}

pub fn pitch_brand(model: &str) -> &'static str {
    registry::find_model_definition(model)
        .map(|d| d.brand)
        .unwrap_or("")
}

pub fn pitch_type_label(model: &str) -> &'static str {
    pitch_model_visual(model)
        .map(|v| v.type_label)
        .unwrap_or("")
}

pub fn pitch_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn validate_pitch_params(model: &str, params: &ParameterSet) -> Result<()> {
    let schema = pitch_model_schema(model)?;
    params
        .normalized_against(&schema)
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!(error))
}

pub fn build_pitch_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_pitch_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Stereo)
}

pub fn build_pitch_processor_for_layout(
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
    anyhow::bail!("unsupported pitch model '{}'", model)
}

pub fn is_pitch_model_available(model: &str) -> bool {
    registry::is_model_available(model)
}

/// Returns the catalog thumbnail path (relative to project root) for a model,
/// or `None` if the model has no thumbnail registered.
pub fn pitch_thumbnail(model: &str) -> Option<&'static str> {
    registry::THUMBNAILS
        .iter()
        .find(|(id, _)| *id == model)
        .map(|(_, path)| *path)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
