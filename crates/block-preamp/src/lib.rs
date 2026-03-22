//! Amplifier models backed by reusable NAM/IR infrastructure.
pub mod native_core;
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreampBackendKind {
    Nam,
    Ir,
    Native,
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
        PreampBackendKind::Native => "native",
        PreampBackendKind::Nam => "NAM",
        PreampBackendKind::Ir => "IR",
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
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}

#[cfg(test)]
mod tests {
    use super::{preamp_model_schema, supported_models};

    #[test]
    fn supported_preamp_models_expose_valid_schema() {
        for model in supported_models() {
            let schema = preamp_model_schema(model).expect("schema should exist");
            assert_eq!(schema.model, *model);
            assert_eq!(schema.effect_type, "preamp");
            assert!(!schema.parameters.is_empty(), "model '{model}' should expose parameters");
        }
    }
}
