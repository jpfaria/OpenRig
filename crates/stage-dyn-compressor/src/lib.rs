//! Compressor implementations.
pub mod studio_clean;

use anyhow::{bail, Result};
use stage_core::MonoProcessor;
use studio_clean::StudioCleanCompressor;

pub const DEFAULT_COMPRESSOR_MODEL: &str = "studio_clean";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompressorParams {
    pub threshold: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_gain_db: f32,
    pub mix: f32,
}

pub fn build_compressor_processor(
    model: &str,
    params: CompressorParams,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    match model {
        DEFAULT_COMPRESSOR_MODEL | "compressor" | "basic" => Ok(Box::new(
            StudioCleanCompressor::new(
            params.threshold,
            params.ratio,
            params.attack_ms,
            params.release_ms,
            params.makeup_gain_db,
            params.mix,
            sample_rate,
        ))),
        other => bail!("unsupported compressor model '{}'", other),
    }
}
