//! Modulation implementations.
pub mod tremolo_sine;

use anyhow::{bail, Result};
use tremolo_sine::{build_processor, model_schema, supports_model};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, StageProcessor};

pub const DEFAULT_TREMOLO_MODEL: &str = "tremolo_sine";

pub fn tremolo_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_model(model) {
        Ok(model_schema())
    } else {
        bail!("unsupported tremolo model '{}'", model)
    }
}

pub fn build_tremolo_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<StageProcessor> {
    build_tremolo_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_tremolo_processor_for_layout(
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
                "tremolo model '{}' is mono-only and cannot build native stereo processing",
                model
            ),
        }
    } else {
        bail!("unsupported tremolo model '{}'", model)
    }
}
