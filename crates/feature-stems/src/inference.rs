//! Stem separation — currently a stub that splits the input into four
//! equal-energy copies.
//!
//! This module is the swap point for the real `ort` + htdemucs
//! implementation. The contract callers depend on is captured here:
//!
//! - input: interleaved stereo `f32` at 44.1 kHz
//! - output: a 4-element vector of interleaved stereo `f32` buffers,
//!   each the same length as the input, ordered
//!   `[drums, bass, vocals, other]`
//! - summing the four stems sample-by-sample yields the input (modulo
//!   floating point rounding)
//!
//! The stub divides each input sample by 4 across the four stems so
//! the sum-invariant holds. When the ML model lands the file is
//! swapped wholesale, the contract above stays intact, and the
//! pipeline/orchestrator does not change.

use crate::StemError;

/// Number of stems produced by the canonical Demucs v4 (htdemucs)
/// model and emulated by this stub.
pub const STEM_COUNT: usize = 4;

/// Separate `input` (stereo interleaved `f32` @ 44.1 kHz) into
/// [`STEM_COUNT`] equal-length stereo buffers.
///
/// # Errors
///
/// - [`StemError::Resample`] when the input length is odd (stereo
///   stays even-length end-to-end).
pub fn separate_stems(input: &[f32], _sample_rate: u32) -> Result<Vec<Vec<f32>>, StemError> {
    if !input.len().is_multiple_of(2) {
        return Err(StemError::Resample {
            reason: format!(
                "stub separator requires stereo input, got {} samples",
                input.len()
            ),
        });
    }
    let scale = 1.0_f32 / STEM_COUNT as f32;
    let mut stems = Vec::with_capacity(STEM_COUNT);
    for _ in 0..STEM_COUNT {
        let mut stem = Vec::with_capacity(input.len());
        for &sample in input {
            stem.push(sample * scale);
        }
        stems.push(stem);
    }
    Ok(stems)
}
