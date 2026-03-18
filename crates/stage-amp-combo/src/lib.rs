pub mod bogner_ecstasy;

use anyhow::{bail, Result};
use bogner_ecstasy::{
    asset_summary as bogner_ecstasy_asset_summary,
    build_processor_for_model as build_bogner_ecstasy_processor,
    model_schema as bogner_ecstasy_model_schema,
    supports_model as supports_bogner_ecstasy_model,
    validate_params as validate_bogner_ecstasy_params,
};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, StageProcessor};

pub fn amp_combo_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_bogner_ecstasy_model(model) {
        Ok(bogner_ecstasy_model_schema())
    } else {
        bail!("unsupported amp-combo model '{}'", model)
    }
}

pub fn amp_combo_asset_summary(model: &str, params: &ParameterSet) -> Result<String> {
    if supports_bogner_ecstasy_model(model) {
        bogner_ecstasy_asset_summary(params)
    } else {
        bail!("unsupported amp-combo model '{}'", model)
    }
}

pub fn validate_amp_combo_params(model: &str, params: &ParameterSet) -> Result<()> {
    if supports_bogner_ecstasy_model(model) {
        validate_bogner_ecstasy_params(params)
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
    _sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    if supports_bogner_ecstasy_model(model) {
        build_bogner_ecstasy_processor(params, layout)
    } else {
        bail!("unsupported amp-combo model '{}'", model)
    }
}
