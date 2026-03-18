//! Tremolo modulation implementations.
pub mod sine;

use anyhow::{bail, Result};
use sine::SineTremolo;
use stage_core::MonoProcessor;

pub const DEFAULT_TREMOLO_MODEL: &str = "sine_tremolo";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TremoloParams {
    pub rate_hz: f32,
    pub depth: f32,
}

pub fn build_tremolo_processor(
    model: &str,
    params: TremoloParams,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    match model {
        DEFAULT_TREMOLO_MODEL | "tremolo" | "basic" => Ok(Box::new(SineTremolo::new(
            params.rate_hz,
            params.depth,
            sample_rate,
        ))),
        other => bail!("unsupported tremolo model '{}'", other),
    }
}
