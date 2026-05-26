//! WAV I/O helpers for the offline render driver.
//!
//! All samples are normalized `f32` in `[-1.0, 1.0]` inside the engine path.
//! Disk-side bit depth is `BitDepth` (16-bit PCM, 24-bit PCM, or 32-bit
//! float). Determinism is required: the same logical input must produce a
//! byte-identical WAV — verified by `issue_552_wav_io.rs`.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

/// On-disk sample format for the rendered output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitDepth {
    Bits16,
    Bits24,
    Bits32Float,
}

/// In-memory WAV payload: interleaved `f32` samples normalized to `[-1.0, 1.0]`.
#[derive(Debug, Clone, PartialEq)]
pub struct WavData {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

/// Errors raised by [`read_wav`] / [`write_wav_stereo`].
#[derive(Debug)]
pub enum WavError {
    Io(std::io::Error),
    Format(hound::Error),
}

impl std::fmt::Display for WavError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "wav io error: {e}"),
            Self::Format(e) => write!(f, "wav format error: {e}"),
        }
    }
}

impl std::error::Error for WavError {}

impl From<std::io::Error> for WavError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<hound::Error> for WavError {
    fn from(e: hound::Error) -> Self {
        match e {
            hound::Error::IoError(io) => Self::Io(io),
            other => Self::Format(other),
        }
    }
}

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

/// Write a stereo `f32` frame buffer to a WAV file at the given sample rate
/// and bit depth.
pub fn write_wav_stereo(
    path: &Path,
    frames: &[[f32; 2]],
    sample_rate_hz: u32,
    bit_depth: BitDepth,
) -> Result<(), WavError> {
    let spec = wav_spec_for(sample_rate_hz, bit_depth);
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    let mut wav = hound::WavWriter::new(writer, spec)?;
    match bit_depth {
        BitDepth::Bits16 => {
            for &[l, r] in frames {
                wav.write_sample(f32_to_i16(l))?;
                wav.write_sample(f32_to_i16(r))?;
            }
        }
        BitDepth::Bits24 => {
            for &[l, r] in frames {
                wav.write_sample(f32_to_i24(l))?;
                wav.write_sample(f32_to_i24(r))?;
            }
        }
        BitDepth::Bits32Float => {
            for &[l, r] in frames {
                wav.write_sample(l)?;
                wav.write_sample(r)?;
            }
        }
    }
    wav.finalize()?;
    Ok(())
}

fn wav_spec_for(sample_rate_hz: u32, bit_depth: BitDepth) -> hound::WavSpec {
    match bit_depth {
        BitDepth::Bits16 => hound::WavSpec {
            channels: 2,
            sample_rate: sample_rate_hz,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
        BitDepth::Bits24 => hound::WavSpec {
            channels: 2,
            sample_rate: sample_rate_hz,
            bits_per_sample: 24,
            sample_format: hound::SampleFormat::Int,
        },
        BitDepth::Bits32Float => hound::WavSpec {
            channels: 2,
            sample_rate: sample_rate_hz,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    }
}

#[inline]
fn f32_to_i16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

#[inline]
fn f32_to_i24(s: f32) -> i32 {
    // 24-bit signed PCM stored in i32 channel by hound.
    (s.clamp(-1.0, 1.0) * 8_388_607.0_f32).round() as i32
}

/// Broadcast a mono `f32` buffer into stereo frames where `L == R`.
///
/// Required by the engine's "always-stereo internal bus" invariant
/// (CLAUDE.md invariant 5).
pub fn broadcast_mono_to_stereo(mono: &[f32]) -> Vec<[f32; 2]> {
    mono.iter().map(|&s| [s, s]).collect()
}

/// Convert an interleaved sample buffer into stereo frames, broadcasting
/// mono inputs and pairing up samples for stereo inputs.
///
/// `channels > 2` is collapsed to stereo by taking the first two channels
/// per frame (consistent with how the live rig handles >2-ch devices today).
pub fn interleaved_to_stereo_frames(interleaved: &[f32], channels: u16) -> Vec<[f32; 2]> {
    match channels {
        1 => broadcast_mono_to_stereo(interleaved),
        2 => interleaved.chunks_exact(2).map(|c| [c[0], c[1]]).collect(),
        n => {
            let n = n as usize;
            interleaved.chunks_exact(n).map(|c| [c[0], c[1]]).collect()
        }
    }
}
