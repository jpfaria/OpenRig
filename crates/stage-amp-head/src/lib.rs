//! Amplifier models backed by reusable NAM/IR infrastructure.
pub mod marshall_jcm_800;

use anyhow::{bail, Result};
use marshall_jcm_800::{
    asset_summary as marshall_jcm_800_asset_summary,
    build_processor_for_model as build_j800_processor, model_schema as j800_model_schema,
    supports_model as supports_j800_model, validate_params as validate_marshall_jcm_800_params,
};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, StageProcessor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmpHeadBackendKind {
    Nam,
    Ir,
}

pub fn amp_head_backend_kind(model: &str) -> Result<AmpHeadBackendKind> {
    if supports_j800_model(model) {
        Ok(AmpHeadBackendKind::Nam)
    } else {
        bail!("unsupported amp-head model '{}'", model)
    }
}

pub fn amp_head_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_j800_model(model) {
        Ok(j800_model_schema())
    } else {
        bail!("unsupported amp-head model '{}'", model)
    }
}

pub fn amp_head_asset_summary(model: &str, params: &ParameterSet) -> Result<String> {
    if supports_j800_model(model) {
        marshall_jcm_800_asset_summary(params)
    } else {
        bail!("unsupported amp-head model '{}'", model)
    }
}

pub fn validate_amp_head_params(model: &str, params: &ParameterSet) -> Result<()> {
    if supports_j800_model(model) {
        validate_marshall_jcm_800_params(params)
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
    _sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    if supports_j800_model(model) {
        build_j800_processor(params, layout)
    } else {
        bail!("unsupported amp-head model '{}'", model)
    }
}
