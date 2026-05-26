//! Real htdemucs inference via ONNX Runtime (`ort`).
//!
//! Gated behind the `real-htdemucs` feature so the default OpenRig
//! build stays slim. When the user drops an htdemucs ONNX model into
//! `<data-dir>/OpenRig/models/htdemucs/htdemucs.onnx` (see
//! `docs/tracks-stems.md` for the conversion recipe), the pipeline
//! picks it up automatically.
//!
//! Input contract — must match the export from `scripts/convert-
//! htdemucs-to-onnx.py`: `(batch=1, channels=2, samples)` float32 at
//! 44.1 kHz.
//!
//! Output contract: `(batch=1, stems=4, channels=2, samples)` float32
//! with stem index `[drums, bass, vocals, other]`.

use std::path::Path;

use ndarray::Array;
use ort::session::Session;
use ort::value::Tensor;

use crate::inference::STEM_COUNT;
use crate::StemError;

const MODEL_SAMPLE_RATE: u32 = 44_100;
const CHANNELS: usize = 2;
/// htdemucs trains on ~7.8s segments. We use a smaller chunk here
/// (~5s) so memory pressure on consumer hardware stays bounded; the
/// model is fully convolutional + transformer so any chunk length
/// works.
const CHUNK_FRAMES: usize = MODEL_SAMPLE_RATE as usize * 5;

/// Run htdemucs inference on `samples` (interleaved stereo `f32` @ 44.1
/// kHz) using the ONNX model at `model_path`. Returns the canonical
/// four stems each as interleaved stereo `f32` of the same length as
/// the input.
///
/// Processing is chunked into `CHUNK_FRAMES`-frame windows with the
/// last (partial) chunk padded with zeros and trimmed back to the
/// real length before concatenation. No overlap-add yet — adding a
/// crossfade is a follow-up polish, not a correctness requirement.
///
/// # Errors
///
/// - [`StemError::Inference`] when loading or running the session
///   fails, or when the output tensor shape does not match the spec.
/// - [`StemError::Resample`] when the input length is odd (stereo
///   stays even-length end-to-end).
pub fn separate_stems_with_ort(
    samples: &[f32],
    sample_rate: u32,
    model_path: &Path,
) -> Result<Vec<Vec<f32>>, StemError> {
    if sample_rate != MODEL_SAMPLE_RATE {
        return Err(StemError::Inference {
            reason: format!(
                "htdemucs requires {} Hz, got {sample_rate}",
                MODEL_SAMPLE_RATE
            ),
        });
    }
    if !samples.len().is_multiple_of(2) {
        return Err(StemError::Resample {
            reason: format!("odd interleaved length {}", samples.len()),
        });
    }
    if !model_path.exists() {
        return Err(StemError::ModelDownload {
            reason: format!(
                "htdemucs ONNX model not found at `{}` — run `scripts/convert-htdemucs-to-onnx.py` to generate it",
                model_path.display()
            ),
        });
    }

    let mut session = Session::builder()
        .map_err(|e| StemError::Inference {
            reason: format!("Session::builder: {e}"),
        })?
        .commit_from_file(model_path)
        .map_err(|e| StemError::Inference {
            reason: format!("commit_from_file `{}`: {e}", model_path.display()),
        })?;

    let total_frames = samples.len() / CHANNELS;
    let mut stems: Vec<Vec<f32>> = (0..STEM_COUNT)
        .map(|_| Vec::with_capacity(samples.len()))
        .collect();

    let mut pos = 0;
    while pos < total_frames {
        let end = (pos + CHUNK_FRAMES).min(total_frames);
        let valid = end - pos;

        // De-interleave the chunk into planar [channels][frames].
        let mut planar = vec![0.0_f32; CHANNELS * CHUNK_FRAMES];
        for frame in 0..valid {
            let src = (pos + frame) * CHANNELS;
            planar[frame] = samples[src];
            planar[CHUNK_FRAMES + frame] = samples[src + 1];
        }

        let input = Array::from_shape_vec((1, CHANNELS, CHUNK_FRAMES), planar).map_err(|e| {
            StemError::Inference {
                reason: format!("reshape input: {e}"),
            }
        })?;
        let input_tensor = Tensor::from_array(input).map_err(|e| StemError::Inference {
            reason: format!("Tensor::from_array: {e}"),
        })?;

        let outputs = session
            .run(ort::inputs!["mix" => input_tensor])
            .map_err(|e| StemError::Inference {
                reason: format!("session.run: {e}"),
            })?;

        let output_value = outputs.iter().next().ok_or_else(|| StemError::Inference {
            reason: "model returned no outputs".to_string(),
        })?;
        let extracted =
            output_value
                .1
                .try_extract_tensor::<f32>()
                .map_err(|e| StemError::Inference {
                    reason: format!("try_extract_tensor: {e}"),
                })?;
        let (shape, data) = extracted;
        if shape.len() != 4
            || shape[0] != 1
            || shape[1] as usize != STEM_COUNT
            || shape[2] as usize != CHANNELS
        {
            return Err(StemError::Inference {
                reason: format!(
                    "unexpected output shape {:?} (want [1, {STEM_COUNT}, {CHANNELS}, T])",
                    shape
                ),
            });
        }
        let out_frames = shape[3] as usize;
        let valid_out = valid.min(out_frames);

        // Re-interleave the per-stem planar output, trim to `valid`
        // frames, append to the running stem buffer.
        for stem_idx in 0..STEM_COUNT {
            let stem_offset = stem_idx * CHANNELS * out_frames;
            for frame in 0..valid_out {
                let l = data[stem_offset + frame];
                let r = data[stem_offset + out_frames + frame];
                stems[stem_idx].push(l);
                stems[stem_idx].push(r);
            }
        }

        pos = end;
    }

    Ok(stems)
}
