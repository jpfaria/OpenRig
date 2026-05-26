//! Sample-rate conversion: interleaved stereo `f32` from any rate to any rate.

use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

use crate::StemError;

const CHUNK_FRAMES: usize = 1024;
const SINC_LEN: usize = 128;
const OVERSAMPLING: usize = 128;

/// Resample interleaved stereo `f32` from `from_sr` to `to_sr`.
///
/// Empty input passes through. When `from_sr == to_sr` the input is
/// cloned verbatim. Odd-length input is rejected — callers must keep
/// the OpenRig stereo invariant (even length, `[L, R, L, R, ...]`).
///
/// # Errors
///
/// - [`StemError::Resample`] when input length is odd or the rubato
///   resampler fails for any reason.
pub fn resample_to(input: &[f32], from_sr: u32, to_sr: u32) -> Result<Vec<f32>, StemError> {
    if !input.len().is_multiple_of(2) {
        return Err(StemError::Resample {
            reason: format!(
                "odd interleaved length {} (stereo requires even length)",
                input.len()
            ),
        });
    }
    if input.is_empty() {
        return Ok(Vec::new());
    }
    if from_sr == to_sr {
        return Ok(input.to_vec());
    }

    let frames = input.len() / 2;
    let (left, right) = deinterleave_stereo(input, frames);

    let params = SincInterpolationParameters {
        sinc_len: SINC_LEN,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: OVERSAMPLING,
        window: WindowFunction::BlackmanHarris2,
    };
    let mut resampler =
        SincFixedIn::<f32>::new(to_sr as f64 / from_sr as f64, 2.0, params, CHUNK_FRAMES, 2)
            .map_err(|err| StemError::Resample {
                reason: err.to_string(),
            })?;

    let approx_out_frames = ((frames as f64) * (to_sr as f64) / (from_sr as f64)).ceil() as usize;
    let mut out_left = Vec::with_capacity(approx_out_frames + CHUNK_FRAMES);
    let mut out_right = Vec::with_capacity(approx_out_frames + CHUNK_FRAMES);

    let mut pos = 0;
    while pos + CHUNK_FRAMES <= frames {
        let chunk = [
            &left[pos..pos + CHUNK_FRAMES],
            &right[pos..pos + CHUNK_FRAMES],
        ];
        let out = resampler
            .process(&chunk, None)
            .map_err(|err| StemError::Resample {
                reason: err.to_string(),
            })?;
        out_left.extend_from_slice(&out[0]);
        out_right.extend_from_slice(&out[1]);
        pos += CHUNK_FRAMES;
    }

    if pos < frames {
        let remaining = frames - pos;
        let mut padded_l = vec![0.0_f32; CHUNK_FRAMES];
        let mut padded_r = vec![0.0_f32; CHUNK_FRAMES];
        padded_l[..remaining].copy_from_slice(&left[pos..]);
        padded_r[..remaining].copy_from_slice(&right[pos..]);
        let chunk = [&padded_l[..], &padded_r[..]];
        let out = resampler
            .process(&chunk, None)
            .map_err(|err| StemError::Resample {
                reason: err.to_string(),
            })?;
        // Only keep output proportional to the real (non-padded) portion.
        let valid =
            ((out[0].len() as f64) * (remaining as f64) / (CHUNK_FRAMES as f64)).round() as usize;
        let valid = valid.min(out[0].len());
        out_left.extend_from_slice(&out[0][..valid]);
        out_right.extend_from_slice(&out[1][..valid]);
    }

    Ok(interleave_stereo(&out_left, &out_right))
}

fn deinterleave_stereo(input: &[f32], frames: usize) -> (Vec<f32>, Vec<f32>) {
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    for pair in input.chunks_exact(2) {
        left.push(pair[0]);
        right.push(pair[1]);
    }
    (left, right)
}

fn interleave_stereo(left: &[f32], right: &[f32]) -> Vec<f32> {
    let len = left.len().min(right.len());
    let mut out = Vec::with_capacity(len * 2);
    for i in 0..len {
        out.push(left[i]);
        out.push(right[i]);
    }
    out
}
