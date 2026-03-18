//! Tuner implementations.
pub mod chromatic;

use anyhow::{bail, Result};
use chromatic::ChromaticTuner;

pub const DEFAULT_TUNER_MODEL: &str = "chromatic_basic";

pub fn build_tuner_processor(
    model: &str,
    _reference_hz: f32,
    sample_rate: usize,
) -> Result<ChromaticTuner> {
    match model {
        DEFAULT_TUNER_MODEL | "chromatic" => {
            let (mut tuner, _handle) = ChromaticTuner::new(sample_rate);
            tuner.set_enabled(true);
            Ok(tuner)
        }
        other => bail!("unsupported tuner model '{}'", other),
    }
}
