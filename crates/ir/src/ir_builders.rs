//! Responsibility: builds the processor a WAV impulse response turns into.

use anyhow::{bail, Result};
use block_core::{MonoProcessor, StereoProcessor};

use crate::ir_asset::{IrAsset, IrChannelData};
use crate::ir_prepare::{resample_if_needed, truncate_with_fade};
use crate::ir_processors::{MonoIrProcessor, StereoIrProcessor};

pub fn build_mono_ir_processor_from_wav(
    path: &str,
    runtime_sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    let ir = IrAsset::load_from_wav(path)?;
    if ir.channel_count() != 1 {
        bail!("IR '{}' is not mono", path);
    }
    let IrChannelData::Mono(samples) = ir.channel_data else {
        unreachable!()
    };
    let samples = truncate_with_fade(samples, path);
    let samples = resample_if_needed(samples, ir.sample_rate, runtime_sample_rate, path);
    Ok(Box::new(MonoIrProcessor::new(samples)?))
}

pub fn build_stereo_ir_processor_from_wav(
    path: &str,
    runtime_sample_rate: f32,
) -> Result<Box<dyn StereoProcessor>> {
    let ir = IrAsset::load_from_wav(path)?;
    if ir.channel_count() != 2 {
        bail!("IR '{}' is not stereo", path);
    }
    let IrChannelData::Stereo(left, right) = ir.channel_data else {
        unreachable!()
    };
    let left = truncate_with_fade(left, path);
    let right = truncate_with_fade(right, path);
    let left = resample_if_needed(left, ir.sample_rate, runtime_sample_rate, path);
    let right = resample_if_needed(right, ir.sample_rate, runtime_sample_rate, path);
    Ok(Box::new(StereoIrProcessor::new(left, right)?))
}
