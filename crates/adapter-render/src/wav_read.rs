//! Responsibility: reads a WAV file into samples.

use std::fs::File;
use std::path::Path;

use crate::wav_types::{WavData, WavError};

/// Read a WAV file as interleaved `f32` samples in `[-1.0, 1.0]`.
///
/// Supports 8/16/24/32-bit integer PCM and 32-bit float. Any bit depth on
/// disk is normalized to `f32` before returning, so the engine path doesn't
/// have to branch.
pub fn read_wav(path: &Path) -> Result<WavData, WavError> {
    let reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let samples = match spec.sample_format {
        hound::SampleFormat::Int => read_int_samples(reader, spec.bits_per_sample)?,
        hound::SampleFormat::Float => read_float_samples(reader)?,
    };
    Ok(WavData {
        sample_rate_hz: spec.sample_rate,
        channels: spec.channels,
        samples,
    })
}

fn read_int_samples(
    mut reader: hound::WavReader<std::io::BufReader<File>>,
    bits_per_sample: u16,
) -> Result<Vec<f32>, WavError> {
    let scale = match bits_per_sample {
        8 => i8::MAX as f32,
        16 => i16::MAX as f32,
        24 => 8_388_607.0_f32,
        32 => i32::MAX as f32,
        _ => return Err(WavError::Format(hound::Error::Unsupported)),
    };
    let mut out = Vec::with_capacity(reader.len() as usize);
    for s in reader.samples::<i32>() {
        out.push((s? as f32) / scale);
    }
    Ok(out)
}

fn read_float_samples(
    mut reader: hound::WavReader<std::io::BufReader<File>>,
) -> Result<Vec<f32>, WavError> {
    let mut out = Vec::with_capacity(reader.len() as usize);
    for s in reader.samples::<f32>() {
        out.push(s?);
    }
    Ok(out)
}
