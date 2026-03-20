//! Amplifier models backed by reusable NAM/IR infrastructure.
pub mod native_core;
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmpHeadBackendKind {
    Nam,
    Ir,
    Native,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn amp_head_backend_kind(model: &str) -> Result<AmpHeadBackendKind> {
    Ok(registry::find_model_definition(model)?.backend_kind)
}

pub fn amp_head_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn amp_head_asset_summary(model: &str, params: &ParameterSet) -> Result<String> {
    (registry::find_model_definition(model)?.asset_summary)(params)
}

pub fn validate_amp_head_params(model: &str, params: &ParameterSet) -> Result<()> {
    (registry::find_model_definition(model)?.validate)(params)
}

pub fn build_amp_head_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_amp_head_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_amp_head_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}

#[cfg(test)]
mod tests {
    use super::{amp_head_model_schema, supported_models};

    #[test]
    fn supported_amp_head_models_expose_valid_schema() {
        for model in supported_models() {
            let schema = amp_head_model_schema(model).expect("schema should exist");
            assert_eq!(schema.model, *model);
            assert!(!schema.parameters.is_empty(), "model '{model}' should expose parameters");
        }
    }
}
