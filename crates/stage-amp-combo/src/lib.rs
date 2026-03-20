pub mod bogner_ecstasy;
pub mod native;

use anyhow::{bail, Result};
use bogner_ecstasy::{
    asset_summary as bogner_ecstasy_asset_summary,
    build_processor_for_model as build_bogner_ecstasy_processor,
    model_schema as bogner_ecstasy_model_schema, supports_model as supports_bogner_ecstasy_model,
    validate_params as validate_bogner_ecstasy_params,
};
use native::{
    asset_summary as native_amp_combo_asset_summary,
    build_processor_for_model as build_native_amp_combo_processor,
    model_schema as native_amp_combo_model_schema, supports_model as supports_native_amp_combo,
    validate_params as validate_native_amp_combo_params,
};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, StageProcessor};

pub fn amp_combo_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_bogner_ecstasy_model(model) {
        Ok(bogner_ecstasy_model_schema())
    } else if supports_native_amp_combo(model) {
        native_amp_combo_model_schema(model)
    } else {
        bail!("unsupported amp-combo model '{}'", model)
    }
}

pub fn amp_combo_asset_summary(model: &str, params: &ParameterSet) -> Result<String> {
    if supports_bogner_ecstasy_model(model) {
        bogner_ecstasy_asset_summary(params)
    } else if supports_native_amp_combo(model) {
        native_amp_combo_asset_summary(model, params)
    } else {
        bail!("unsupported amp-combo model '{}'", model)
    }
}

pub fn validate_amp_combo_params(model: &str, params: &ParameterSet) -> Result<()> {
    if supports_bogner_ecstasy_model(model) {
        validate_bogner_ecstasy_params(params)
    } else if supports_native_amp_combo(model) {
        validate_native_amp_combo_params(model, params)
    } else {
        bail!("unsupported amp-combo model '{}'", model)
    }
}

pub fn build_amp_combo_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<StageProcessor> {
    build_amp_combo_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_amp_combo_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    if supports_bogner_ecstasy_model(model) {
        build_bogner_ecstasy_processor(params, layout)
    } else if supports_native_amp_combo(model) {
        build_native_amp_combo_processor(model, params, sample_rate, layout)
    } else {
        bail!("unsupported amp-combo model '{}'", model)
    }
}

#[cfg(test)]
mod tests {
    use super::{amp_combo_model_schema, build_amp_combo_processor_for_layout};
    use stage_core::param::ParameterSet;
    use stage_core::{AudioChannelLayout, ModelAudioMode};

    #[test]
    fn native_amp_combo_catalog_is_publicly_supported() {
        for model in [
            "blackface_clean_combo",
            "tweed_breakup_combo",
            "chime_combo",
        ] {
            let schema = amp_combo_model_schema(model).expect("schema should exist");
            assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
            assert_eq!(schema.parameters.len(), 10);
        }
    }

    #[test]
    fn native_amp_combos_build_for_stereo_tracks() {
        for model in [
            "blackface_clean_combo",
            "tweed_breakup_combo",
            "chime_combo",
        ] {
            let schema = amp_combo_model_schema(model).expect("schema should exist");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("defaults should normalize");

            let processor = build_amp_combo_processor_for_layout(
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
