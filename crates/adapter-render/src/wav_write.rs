//! Responsibility: writes stereo samples out as a WAV file.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use crate::wav_types::{BitDepth, WavError};

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
