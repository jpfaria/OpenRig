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
/// model and emulated by this stub. `htdemucs_6s` produces 6 — the
/// ort path reads the real count from the model output; the stub
/// stays at 4.
pub const STEM_COUNT: usize = 4;

/// Separate `input` (stereo interleaved `f32` @ 44.1 kHz) into
/// [`STEM_COUNT`] equal-length stereo buffers.
///
/// Stub strategy: cheap per-sample one-pole filter bank that routes
/// audible energy to each stem so the user actually HEARS a
/// difference between solo'd stems while the real htdemucs path is
/// not enabled. Roughly:
///
/// - `drums`  — band-pass around 80–400 Hz (kick + snare body)
/// - `bass`   — low-pass below 200 Hz
/// - `vocals` — band-pass around 300–3 kHz (formant range)
/// - `other`  — residual high frequencies (1.5 kHz+)
///
/// The bands intentionally overlap so the sum still reconstructs
/// most of the original energy. NOT a real source separator — that
/// is `feature-stems/real-htdemucs` + an ONNX model.
///
/// # Errors
///
/// - [`StemError::Resample`] when the input length is odd (stereo
///   stays even-length end-to-end).
pub fn separate_stems(input: &[f32], sample_rate: u32) -> Result<Vec<Vec<f32>>, StemError> {
    if !input.len().is_multiple_of(2) {
        return Err(StemError::Resample {
            reason: format!(
                "stub separator requires stereo input, got {} samples",
                input.len()
            ),
        });
    }

    let sr = sample_rate.max(1) as f32;
    let alpha_low = one_pole_alpha(200.0, sr);
    let alpha_low_band_hp = one_pole_alpha(80.0, sr);
    let alpha_mid_lp = one_pole_alpha(400.0, sr);
    let alpha_vox_hp = one_pole_alpha(300.0, sr);
    let alpha_vox_lp = one_pole_alpha(3_000.0, sr);
    let alpha_other_hp = one_pole_alpha(1_500.0, sr);

    let frames = input.len() / 2;
    let mut stems: Vec<Vec<f32>> = (0..STEM_COUNT)
        .map(|_| Vec::with_capacity(input.len()))
        .collect();

    // Per-channel filter state (left / right) for every band that
    // needs memory across samples.
    let mut state = StubState::default();

    for f in 0..frames {
        let l = input[f * 2];
        let r = input[f * 2 + 1];

        // bass: low-pass < 200 Hz
        state.bass_l = lerp(state.bass_l, l, alpha_low);
        state.bass_r = lerp(state.bass_r, r, alpha_low);
        let bass_l = state.bass_l;
        let bass_r = state.bass_r;

        // drums: high-pass at 80 Hz THEN low-pass at 400 Hz (band-pass)
        state.drums_hp_l = lerp(state.drums_hp_l, l, alpha_low_band_hp);
        state.drums_hp_r = lerp(state.drums_hp_r, r, alpha_low_band_hp);
        let drums_hp_l = l - state.drums_hp_l;
        let drums_hp_r = r - state.drums_hp_r;
        state.drums_lp_l = lerp(state.drums_lp_l, drums_hp_l, alpha_mid_lp);
        state.drums_lp_r = lerp(state.drums_lp_r, drums_hp_r, alpha_mid_lp);
        let drums_l = state.drums_lp_l;
        let drums_r = state.drums_lp_r;

        // vocals: 300 Hz – 3 kHz band
        state.vox_hp_l = lerp(state.vox_hp_l, l, alpha_vox_hp);
        state.vox_hp_r = lerp(state.vox_hp_r, r, alpha_vox_hp);
        let vox_hp_l = l - state.vox_hp_l;
        let vox_hp_r = r - state.vox_hp_r;
        state.vox_lp_l = lerp(state.vox_lp_l, vox_hp_l, alpha_vox_lp);
        state.vox_lp_r = lerp(state.vox_lp_r, vox_hp_r, alpha_vox_lp);
        let vox_l = state.vox_lp_l;
        let vox_r = state.vox_lp_r;

        // other: anything above 1.5 kHz
        state.other_l = lerp(state.other_l, l, alpha_other_hp);
        state.other_r = lerp(state.other_r, r, alpha_other_hp);
        let other_l = l - state.other_l;
        let other_r = r - state.other_r;

        stems[0].push(drums_l);
        stems[0].push(drums_r);
        stems[1].push(bass_l);
        stems[1].push(bass_r);
        stems[2].push(vox_l);
        stems[2].push(vox_r);
        stems[3].push(other_l);
        stems[3].push(other_r);
    }

    Ok(stems)
}

#[derive(Default)]
struct StubState {
    bass_l: f32,
    bass_r: f32,
    drums_hp_l: f32,
    drums_hp_r: f32,
    drums_lp_l: f32,
    drums_lp_r: f32,
    vox_hp_l: f32,
    vox_hp_r: f32,
    vox_lp_l: f32,
    vox_lp_r: f32,
    other_l: f32,
    other_r: f32,
}

fn one_pole_alpha(cutoff_hz: f32, sample_rate_hz: f32) -> f32 {
    let dt = 1.0_f32 / sample_rate_hz;
    let rc = 1.0_f32 / (std::f32::consts::TAU * cutoff_hz.max(1.0));
    dt / (rc + dt)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
