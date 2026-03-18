//! Digital delay implementations.
pub mod basic;

use anyhow::{bail, Result};
use basic::BasicDelay;
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DelayParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
}

impl Default for DelayParams {
    fn default() -> Self {
        Self {
            time_ms: 380.0,
            feedback: 0.35,
            mix: 0.3,
        }
    }
}

pub fn build_delay_processor(
    model: &str,
    params: DelayParams,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    match model {
        DEFAULT_DELAY_MODEL | "native_digital" | "rust_style_digital" | "digital" => {
            Ok(Box::new(BasicDelay::new(params, sample_rate)))
        }
        other => bail!("unsupported delay model '{}'", other),
    }
}
