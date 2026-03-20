//! Reverb implementations.
pub mod plate_foundation;

use anyhow::{bail, Result};
use plate_foundation::{build_processor, model_schema, supports_model};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, NamedModel, StageProcessor};

pub enum ReverbModel {
    PlateFoundation,
}

impl NamedModel for ReverbModel {
    fn model_key(&self) -> &'static str {
        match self {
            ReverbModel::PlateFoundation => plate_foundation::MODEL_ID,
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            ReverbModel::PlateFoundation => "Plate Foundation Reverb",
        }
    }
}

pub fn supported_models() -> &'static [&'static str] {
    &[plate_foundation::MODEL_ID]
}

pub fn reverb_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_model(model) {
        Ok(model_schema())
    } else {
        bail!("unsupported reverb model '{}'", model)
    }
}

pub fn build_reverb_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<StageProcessor> {
    build_reverb_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_reverb_processor_for_layout(
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
                "reverb model '{}' is mono-only and cannot build native stereo processing",
                model
            ),
        }
    } else {
        bail!("unsupported reverb model '{}'", model)
    }
}
