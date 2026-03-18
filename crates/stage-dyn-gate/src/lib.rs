//! Noise gate implementations.
pub mod basic;

use anyhow::{bail, Result};
use basic::BasicNoiseGate;
use stage_core::MonoProcessor;

pub const DEFAULT_GATE_MODEL: &str = "noise_gate_basic";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GateParams {
    pub threshold: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
}

pub fn build_gate_processor(
    model: &str,
    params: GateParams,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    match model {
        DEFAULT_GATE_MODEL | "gate" | "basic" => Ok(Box::new(BasicNoiseGate::new(
            params.threshold,
            params.attack_ms,
            params.release_ms,
            sample_rate,
        ))),
        other => bail!("unsupported gate model '{}'", other),
    }
}
