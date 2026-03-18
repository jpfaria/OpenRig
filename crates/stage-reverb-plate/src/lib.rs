//! Plate reverb implementations.
pub mod foundation;

use anyhow::{bail, Result};
use foundation::{build_processor, model_schema, supports_model};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{MonoProcessor, NamedModel};

pub const DEFAULT_REVERB_MODEL: &str = "plate_foundation";

pub enum ReverbModel {
    PlateFoundation,
}

impl NamedModel for ReverbModel {
    fn model_key(&self) -> &'static str {
        match self {
            ReverbModel::PlateFoundation => DEFAULT_REVERB_MODEL,
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            ReverbModel::PlateFoundation => "Plate Foundation Reverb",
        }
    }
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
) -> Result<Box<dyn MonoProcessor>> {
    if supports_model(model) {
        build_processor(params, sample_rate)
    } else {
        bail!("unsupported reverb model '{}'", model)
    }
}
