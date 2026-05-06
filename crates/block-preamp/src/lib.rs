//! Amplifier models backed by reusable NAM/IR infrastructure.
pub mod model_visual;
pub mod native_core;
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreampBackendKind {
    Nam,
    Ir,
    Native,
    Lv2,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn preamp_backend_kind(model: &str) -> Result<PreampBackendKind> {
    Ok(registry::find_model_definition(model)?.backend_kind)
}

pub fn preamp_display_name(model: &str) -> Result<&'static str> {
    Ok(registry::find_model_definition(model)?.display_name)
}

pub fn preamp_brand(model: &str) -> Result<&'static str> {
    Ok(registry::find_model_definition(model)?.brand)
}

/// Retorna o tipo do modelo como string legível: "native", "NAM" ou "IR"
pub fn preamp_type_label(model: &str) -> Result<&'static str> {
    Ok(match registry::find_model_definition(model)?.backend_kind {
        PreampBackendKind::Native => block_core::BRAND_NATIVE,
        PreampBackendKind::Nam => "NAM",
        PreampBackendKind::Ir => "IR",
        PreampBackendKind::Lv2 => "LV2",
    })
}

pub fn preamp_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            PreampBackendKind::Native => "NATIVE",
            PreampBackendKind::Nam => "NAM",
            PreampBackendKind::Ir => "IR",
            PreampBackendKind::Lv2 => "LV2",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
        thumbnail_path: None,
        available: true,
    })
}

pub fn preamp_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn preamp_asset_summary(model: &str, params: &ParameterSet) -> Result<String> {
    (registry::find_model_definition(model)?.asset_summary)(params)
}

pub fn validate_preamp_params(model: &str, params: &ParameterSet) -> Result<()> {
    (registry::find_model_definition(model)?.validate)(params)
}

pub fn build_preamp_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_preamp_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_preamp_processor_for_layout(
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
    anyhow::bail!("unsupported preamp model '{}'", model)
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
