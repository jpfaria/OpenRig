//! Amplifier models backed by reusable NAM/IR infrastructure.
pub mod marshall_jcm_800;
pub mod native;

use anyhow::{bail, Result};
use marshall_jcm_800::{
    asset_summary as marshall_jcm_800_asset_summary,
    build_processor_for_model as build_j800_processor, model_schema as j800_model_schema,
    supports_model as supports_j800_model, validate_params as validate_marshall_jcm_800_params,
};
use native::{
    asset_summary as native_amp_head_asset_summary,
    build_processor_for_model as build_native_amp_head_processor,
    model_schema as native_amp_head_model_schema, supports_model as supports_native_amp_head_model,
    validate_params as validate_native_amp_head_params,
};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, StageProcessor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmpHeadBackendKind {
    Nam,
    Ir,
    Native,
}

pub fn supported_models() -> &'static [&'static str] {
    &[
        marshall_jcm_800::MODEL_ID,
        native::BRIT_CRUNCH_HEAD_ID,
        native::AMERICAN_CLEAN_HEAD_ID,
        native::MODERN_HIGH_GAIN_HEAD_ID,
    ]
}

pub fn amp_head_backend_kind(model: &str) -> Result<AmpHeadBackendKind> {
    if supports_j800_model(model) {
        Ok(AmpHeadBackendKind::Nam)
    } else if supports_native_amp_head_model(model) {
        Ok(AmpHeadBackendKind::Native)
    } else {
        bail!("unsupported amp-head model '{}'", model)
    }
}

pub fn amp_head_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_j800_model(model) {
        Ok(j800_model_schema())
    } else if supports_native_amp_head_model(model) {
        native_amp_head_model_schema(model)
    } else {
        bail!("unsupported amp-head model '{}'", model)
    }
}

pub fn amp_head_asset_summary(model: &str, params: &ParameterSet) -> Result<String> {
    if supports_j800_model(model) {
        marshall_jcm_800_asset_summary(params)
    } else if supports_native_amp_head_model(model) {
        native_amp_head_asset_summary(model, params)
    } else {
        bail!("unsupported amp-head model '{}'", model)
    }
}

pub fn validate_amp_head_params(model: &str, params: &ParameterSet) -> Result<()> {
    if supports_j800_model(model) {
        validate_marshall_jcm_800_params(params)
    } else if supports_native_amp_head_model(model) {
        validate_native_amp_head_params(model, params)
    } else {
        bail!("unsupported amp-head model '{}'", model)
    }
}

pub fn build_amp_head_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<StageProcessor> {
    build_amp_head_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_amp_head_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    if supports_j800_model(model) {
        build_j800_processor(params, layout)
    } else if supports_native_amp_head_model(model) {
        build_native_amp_head_processor(model, params, sample_rate, layout)
    } else {
        bail!("unsupported amp-head model '{}'", model)
    }
}

#[cfg(test)]
mod tests {
    use super::{amp_head_model_schema, build_amp_head_processor_for_layout};
    use stage_core::param::ParameterSet;
    use stage_core::{AudioChannelLayout, ModelAudioMode};

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
    fn native_amp_heads_build_for_stereo_tracks() {
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
                "expected '{model}' to build for stereo tracks"
            );
        }
    }
}
