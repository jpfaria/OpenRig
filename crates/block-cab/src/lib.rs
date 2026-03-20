pub mod native_core;
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CabBackendKind {
    Ir,
    Native,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn cab_backend_kind(model: &str) -> Result<CabBackendKind> {
    Ok(registry::find_model_definition(model)?.backend_kind)
}

pub fn cab_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn cab_asset_summary(model: &str, params: &ParameterSet) -> Result<String> {
    (registry::find_model_definition(model)?.asset_summary)(params)
}

pub fn validate_cab_params(model: &str, params: &ParameterSet) -> Result<()> {
    (registry::find_model_definition(model)?.validate)(params)
}

pub fn build_cab_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_cab_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_cab_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}

#[cfg(test)]
mod tests {
    use block_core::param::ParameterSet;
    use block_core::AudioChannelLayout;
    use crate::{build_cab_processor_for_layout, cab_backend_kind, cab_model_schema, supported_models, CabBackendKind};

    #[test]
    fn supported_cabs_expose_valid_schema() {
        for model in supported_models() {
            let schema = cab_model_schema(model).expect("cab schema should exist");
            assert_eq!(schema.model, *model);
            assert!(!schema.parameters.is_empty(), "model '{model}' should expose parameters");
        }
    }

    #[test]
    fn supported_cabs_build_for_mono_chains() {
        for model in supported_models() {
            let schema = cab_model_schema(model).expect("schema should exist");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("defaults should normalize");

            let processor = build_cab_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Mono,
            );

            assert!(processor.is_ok(), "expected '{model}' to build for mono chains");
        }
    }

    #[test]
    fn native_cabs_build_for_stereo_chains() {
        for model in supported_models() {
            if !matches!(cab_backend_kind(model).expect("backend"), CabBackendKind::Native) {
                continue;
            }
            let schema = cab_model_schema(model).expect("schema should exist");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("defaults should normalize");

            let processor = build_cab_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Stereo,
            );

            assert!(
                processor.is_ok(),
                "expected '{model}' to build for stereo chains"
            );
        }
    }
}
