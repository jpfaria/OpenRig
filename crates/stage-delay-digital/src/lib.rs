//! Digital delay implementations.
pub mod basic;

use anyhow::{bail, Result};
use basic::{build_processor, model_schema, supports_model};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{MonoProcessor, NamedModel};

pub const DEFAULT_DELAY_MODEL: &str = "digital_basic";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelayModel {
    DigitalBasic,
}

impl NamedModel for DelayModel {
    fn model_key(&self) -> &'static str {
        match self {
            DelayModel::DigitalBasic => DEFAULT_DELAY_MODEL,
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            DelayModel::DigitalBasic => "Digital Basic Delay",
        }
    }
}

pub fn delay_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_model(model) {
        Ok(model_schema())
    } else {
        bail!("unsupported delay model '{}'", model)
    }
}

pub fn build_delay_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    if supports_model(model) {
        build_processor(params, sample_rate)
    } else {
        bail!("unsupported delay model '{}'", model)
    }
}
