//! Amplifier models backed by reusable NAM/IR infrastructure.
pub mod j800;

use anyhow::{bail, Result};
use j800::{
    build_processor_for_model as build_j800_processor, model_schema as j800_model_schema,
    supports_model as supports_j800_model,
};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, StageProcessor};

pub const DEFAULT_AMP_MODEL: &str = j800::MODEL_ID;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmpBackendKind {
    Nam,
    Ir,
}

pub fn amp_backend_kind(model: &str) -> Result<AmpBackendKind> {
    if supports_j800_model(model) {
        Ok(AmpBackendKind::Nam)
    } else {
        bail!("unsupported amp model '{}'", model)
    }
}

pub fn amp_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_j800_model(model) {
        Ok(j800_model_schema())
    } else {
        bail!("unsupported amp model '{}'", model)
    }
}

pub fn build_amp_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<StageProcessor> {
    build_amp_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_amp_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    _sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    if supports_j800_model(model) {
        build_j800_processor(params, layout)
    } else {
        bail!("unsupported amp model '{}'", model)
    }
}
