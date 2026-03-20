//! Amplifier models backed by reusable NAM/IR infrastructure.
pub mod marshall_jcm_800;
pub mod native;
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
    use super::{amp_head_model_schema, build_amp_head_processor_for_layout};
    use block_core::param::ParameterSet;
    use block_core::{AudioChannelLayout, ModelAudioMode};

    #[test]
    fn native_amp_head_catalog_is_publicly_supported() {
        for model in [
            "brit_crunch_head",
            "american_clean_head",
            "modern_high_gain_head",
        ] {
            let schema = amp_head_model_schema(model).expect("schema should exist");
            assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
            assert_eq!(schema.parameters.len(), 11);
        }
    }

    #[test]
    fn native_amp_heads_build_for_stereo_chains() {
        for model in [
            "brit_crunch_head",
            "american_clean_head",
            "modern_high_gain_head",
        ] {
            let schema = amp_head_model_schema(model).expect("schema should exist");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("defaults should normalize");

            let processor = build_amp_head_processor_for_layout(
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
