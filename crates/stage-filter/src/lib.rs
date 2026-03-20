//! Filter implementations.
pub mod eq_three_band_basic;

use anyhow::{bail, Result};
use eq_three_band_basic::{build_processor, model_schema, supports_model};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, StageProcessor};

pub fn supported_models() -> &'static [&'static str] {
    &[eq_three_band_basic::MODEL_ID]
}

pub fn eq_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_model(model) {
        Ok(model_schema())
    } else {
        bail!("unsupported eq model '{}'", model)
    }
}

pub fn build_eq_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<StageProcessor> {
    build_eq_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_eq_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    if supports_model(model) {
        match layout {
            AudioChannelLayout::Mono => {
                Ok(StageProcessor::Mono(build_processor(params, sample_rate)?))
            }
            AudioChannelLayout::Stereo => bail!(
                "eq model '{}' is mono-only and cannot build native stereo processing",
                model
            ),
        }
    } else {
        bail!("unsupported eq model '{}'", model)
    }
}
