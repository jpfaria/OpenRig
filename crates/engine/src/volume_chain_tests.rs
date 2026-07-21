//! Volume/audio invariants — PINNED (issue #792 split from volume_invariants_tests.rs).
//! Section moved verbatim; shared fixtures live in `volume_invariants_tests.rs`.
#![allow(unused_imports)]
use super::*;
use super::volume_invariants::*;

// ─────────────────────────────────────────────────────────────────────────
// A. Layout passthrough — every Input mode × Output mode combo
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn a01_mono_in_stereo_out_broadcasts_at_unity() {
    let (chain, registry) = chain_with_blocks(
        "a01",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peaks = measure_steady_per_channel_peak(&chain, &registry, 1, &[0.5], 2, 4);
    assert!(
        (peaks[0] - 0.5).abs() < TOLERANCE,
        "L peak expected ≈ 0.5, got {}",
        peaks[0]
    );
    assert!(
        (peaks[1] - 0.5).abs() < TOLERANCE,
        "R peak expected ≈ 0.5, got {}",
        peaks[1]
    );
}

#[test]
fn a02_mono_in_mono_out_passes_through_at_unity() {
    let (chain, registry) = chain_with_blocks(
        "a02",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Mono, vec![0]),
    );
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.4], 1, 4);
    assert!(
        (peak - 0.4).abs() < TOLERANCE,
        "mono in → mono out must be unity; got {peak}"
    );
}

#[test]
fn a03_stereo_in_stereo_out_preserves_l_and_r() {
    let (chain, registry) = chain_with_blocks(
        "a03",
        input_stereo(vec![0, 1]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peaks = measure_steady_per_channel_peak(&chain, &registry, 2, &[0.3, 0.6], 2, 4);
    assert!(
        (peaks[0] - 0.3).abs() < TOLERANCE,
        "L expected ≈ 0.3, got {}",
        peaks[0]
    );
    assert!(
        (peaks[1] - 0.6).abs() < TOLERANCE,
        "R expected ≈ 0.6, got {}",
        peaks[1]
    );
}

#[test]
fn a04_dual_mono_in_stereo_out_preserves_independent_l_r() {
    let (chain, registry) = chain_with_blocks(
        "a04",
        input_dual_mono(vec![0, 1]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peaks = measure_steady_per_channel_peak(&chain, &registry, 2, &[0.25, 0.75], 2, 4);
    assert!(
        (peaks[0] - 0.25).abs() < TOLERANCE,
        "L expected ≈ 0.25, got {}",
        peaks[0]
    );
    assert!(
        (peaks[1] - 0.75).abs() < TOLERANCE,
        "R expected ≈ 0.75, got {}",
        peaks[1]
    );
}

// ─────────────────────────────────────────────────────────────────────────
// B. Output limiter — soft tanh above 0.95, transparent below
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn b01_output_below_limiter_knee_is_transparent() {
    let (chain, registry) = chain_with_blocks(
        "b01",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.9], 2, 4);
    assert!(
        (peak - 0.9).abs() < TOLERANCE,
        "limiter must be transparent below 0.95; got {peak}"
    );
}

#[test]
fn b02_output_above_limiter_knee_is_softly_saturated() {
    // Issue #496: was `b02_..._applies_tanh` and pinned `peak ≈ tanh(1.5)`.
    // The tanh form was discontinuous (-2.17 dB step at 0.95) and
    // non-monotonic 0.95..1.83 — proven RED in runtime_dsp::tests. The
    // invariant being protected is "above the knee saturates", not the
    // specific math; pin the PROPERTIES instead of the function shape.
    let (chain, registry) = chain_with_blocks(
        "b02",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peak = measure_steady_peak(&chain, &registry, 1, &[1.5], 2, 4);
    assert!(
        peak <= 1.0,
        "above-knee must be bounded ≤ full scale; got {peak}"
    );
    assert!(
        peak < 1.5,
        "above-knee must be reduced from input 1.5; got {peak}"
    );
    assert!(
        peak > 0.9,
        "above-knee must stay loud (no quiet collapse); got {peak}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// C. Volume block — unity / fractional gain control
// ─────────────────────────────────────────────────────────────────────────

/// PIN — volume scale shape (issue #400 bug #3 fix, 2026-05-09):
///   `db = 20 * log10(percent / 100)` (logarithmic taper, floor at -60 dB)
/// resulting in:
///   - 0%   → -60 dB (silence floor)
///   - 25%  → -12 dB
///   - 50%  →  -6 dB                 ← halving = -6 dB (industry standard)
///   - 100% →   0 dB (unity)         ← passthrough; identical to bypass
///
/// **CHANGED FROM PREVIOUS PIN** (linear `db = -60 + percent/100 * 72`):
/// the old mapping had +12 dB headroom above unity at 100%, which caused
/// silent DAC clipping (user report 2026-05-09: "volume 100% deixa o som
/// mais baixo do que com ele desligado"). The fix removes the boost so
/// 100% is exactly unity. User must use a dedicated boost block if extra
/// gain is needed downstream.
///
/// User explicitly authorised this pin update (issue #400) — the
/// `volume_invariants_tests.rs` invariant only forbids changes WITHOUT
/// explicit user request. With request, both the source and the pin
/// move together; subsequent regressions are still caught.
#[test]
fn c01_volume_block_at_80_percent_is_minus_1_94_db() {
    let mut params = neutral_params("gain", "volume");
    params.insert("volume", ParameterValue::Float(80.0));
    params.insert("mute", ParameterValue::Bool(false));
    let (chain, registry) = chain_with_blocks(
        "c01",
        input_mono(vec![0]),
        vec![core_block("vol", "gain", "volume", params)],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.5], 2, 6);
    // 20 * log10(0.8) = -1.938 dB → gain ≈ 0.8 → 0.5 * 0.8 = 0.4
    let gain = 10.0_f32.powf(-1.938 / 20.0);
    let expected = 0.5 * gain;
    assert!(
        (peak - expected).abs() < 0.01,
        "volume at 80% emits -1.94 dB (logarithmic taper); \
         expected ≈ {expected}, got {peak}"
    );
}

/// PIN: unity gain happens at exactly 100% in the new logarithmic
/// implementation, because `20 * log10(1.0) = 0`. This is the
/// industry-standard convention for volume controls.
#[test]
fn c02_volume_block_at_100_percent_is_unity() {
    let mut params = neutral_params("gain", "volume");
    params.insert("volume", ParameterValue::Float(100.0));
    params.insert("mute", ParameterValue::Bool(false));
    let (chain, registry) = chain_with_blocks(
        "c02",
        input_mono(vec![0]),
        vec![core_block("vol", "gain", "volume", params)],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.5], 2, 6);
    assert!(
        (peak - 0.5).abs() < 0.01,
        "volume at 100% is unity passthrough; expected 0.5, got {peak}"
    );
}

/// PIN: 50% on the logarithmic taper produces -6 dB → gain ≈ 0.5x.
/// This is the perceptual "halving" point — a knob at center should
/// sound roughly half as loud as fully open.
#[test]
fn c04_volume_block_at_50_percent_is_minus_6_db() {
    let mut params = neutral_params("gain", "volume");
    params.insert("volume", ParameterValue::Float(50.0));
    params.insert("mute", ParameterValue::Bool(false));
    let (chain, registry) = chain_with_blocks(
        "c04",
        input_mono(vec![0]),
        vec![core_block("vol", "gain", "volume", params)],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.5], 2, 6);
    // 20 * log10(0.5) = -6.02 dB → gain ≈ 0.5012 → 0.5 * 0.5012 = 0.2506
    let expected = 0.5 * 10.0_f32.powf(-6.02 / 20.0);
    assert!(
        (peak - expected).abs() < 0.01,
        "volume at 50% emits -6 dB (perceptual halving); \
         expected ≈ {expected}, got {peak}"
    );
}

#[test]
fn c03_volume_block_muted_emits_silence() {
    let mut params = neutral_params("gain", "volume");
    params.insert("volume", ParameterValue::Float(80.0));
    params.insert("mute", ParameterValue::Bool(true));
    let (chain, registry) = chain_with_blocks(
        "c03",
        input_mono(vec![0]),
        vec![core_block("vol", "gain", "volume", params)],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.5], 2, 6);
    assert!(
        peak < 0.01,
        "muted volume block must emit silence; got {peak}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// D. Tremolo — the user's actual culprit on Mac (2026-04-28)
// ─────────────────────────────────────────────────────────────────────────

/// CONTRACT: tremolo at depth=0 must be a no-op. The block exists in
/// the chain, but no amplitude modulation. Catches "tremolo block
/// silently introduces gain change" regression.
#[test]
fn d01_tremolo_at_zero_depth_is_transparent() {
    let mut params = neutral_params("modulation", "tremolo_sine");
    params.insert("rate_hz", ParameterValue::Float(4.0));
    params.insert("depth", ParameterValue::Float(0.0));
    let (chain, registry) = chain_with_blocks(
        "d01",
        input_mono(vec![0]),
        vec![core_block("trem", "modulation", "tremolo_sine", params)],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.5], 2, 6);
    assert!(
        (peak - 0.5).abs() < TOLERANCE,
        "tremolo depth=0 must be transparent; got {peak}"
    );
}

/// CONTRACT: tremolo at depth=50% modulates between unity and 50%.
/// Peak observed across many callbacks must reach ≈ unity (input level).
#[test]
fn d02_tremolo_at_50_depth_peaks_at_unity() {
    let mut params = neutral_params("modulation", "tremolo_sine");
    params.insert("rate_hz", ParameterValue::Float(4.0));
    params.insert("depth", ParameterValue::Float(50.0));
    let (chain, registry) = chain_with_blocks(
        "d02",
        input_mono(vec![0]),
        vec![core_block("trem", "modulation", "tremolo_sine", params)],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    // Run many callbacks so the tremolo LFO traverses a full cycle.
    let runtime = build_runtime(&chain, &registry);
    let mut all_samples: Vec<f32> = Vec::new();
    for _ in 0..32 {
        let data = const_interleaved(&[0.5], 256);
        let out = drive_and_capture(&runtime, 1, &data, 2);
        all_samples.extend(out);
    }
    // Skip first 1024 samples (fade-in + ramp).
    let steady = &all_samples[1024..];
    let peak = peak_abs(steady);
    assert!(
        (peak - 0.5).abs() < TOLERANCE,
        "tremolo depth=50 must reach unity peak; got {peak}"
    );
}

/// REGRESSION DOC: replicates the user's CLEAN chain on 2026-04-28.
/// Tremolo at depth=50% modulates between unity and 50% of input
/// (signal averages around 75% → -2.5 dB). Pinned: a chain with this
/// exact tremolo config must show modulation, but the peak must still
/// hit unity (i.e. the engine isn't applying an extra gain).
///
/// **NOTE on the user's actual CLEAN chain:** the chain also has
/// `amp blackface_clean` with `master: 100` which applies multiple
/// internal drive stages (see `block_preamp::native_core`). Combined
/// with `output_limiter` tanh, that amp clamps the signal at ~0.86
/// and removes dynamics, so the perceived "low + compressed" sound
/// has TWO sources: the tremolo modulation tested here AND the amp's
/// internal saturation curve at `master: 100`. The amp behavior is
/// documented in `k01_blackface_clean_master_100_internal_saturation`.
#[test]
fn d03_user_clean_chain_tremolo_signature() {
    let mut params = neutral_params("modulation", "tremolo_sine");
    params.insert("rate_hz", ParameterValue::Float(4.0));
    params.insert("depth", ParameterValue::Float(50.0));
    let (chain, registry) = chain_with_blocks(
        "d03",
        input_mono(vec![0]),
        vec![core_block("trem", "modulation", "tremolo_sine", params)],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let runtime = build_runtime(&chain, &registry);
    let mut all_samples: Vec<f32> = Vec::new();
    for _ in 0..40 {
        let data = const_interleaved(&[0.5], 256);
        let out = drive_and_capture(&runtime, 1, &data, 2);
        all_samples.extend(out);
    }
    // Steady-state window past fade-in.
    let steady = &all_samples[1024..];
    let peak = peak_abs(steady);
    let trough = min_abs(steady);
    // Peak ≈ 0.5 (unity), trough ≈ 0.25 (50% depth). If depth is
    // mistakenly normalized to 0.5/100=0.005 or doubled, this fails.
    assert!(
        (peak - 0.5).abs() < TOLERANCE,
        "tremolo signature: peak must reach unity 0.5; got {peak}"
    );
    assert!(
        trough < 0.30 && trough > 0.20,
        "tremolo signature: trough must be ≈ 0.25 (depth=50%); got {trough}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// E. Multi-block composition — neutral blocks in sequence stay at unity
// ─────────────────────────────────────────────────────────────────────────

/// CONTRACT: a chain of multiple blocks each at neutral params must
/// preserve unity gain end-to-end. Catches per-block hidden attenuation.
#[test]
fn e01_two_unity_volume_blocks_preserve_unity() {
    // Issue #400 bug #3: unity is now at 100% (was 83.33% on linear taper).
    let mut p1 = neutral_params("gain", "volume");
    p1.insert("volume", ParameterValue::Float(100.0)); // unity point
    p1.insert("mute", ParameterValue::Bool(false));
    let mut p2 = neutral_params("gain", "volume");
    p2.insert("volume", ParameterValue::Float(100.0));
    p2.insert("mute", ParameterValue::Bool(false));
    let (chain, registry) = chain_with_blocks(
        "e01",
        input_mono(vec![0]),
        vec![
            core_block("v1", "gain", "volume", p1),
            core_block("v2", "gain", "volume", p2),
        ],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.5], 2, 6);
    assert!(
        (peak - 0.5).abs() < 0.01,
        "two unity-volume (100%) blocks must preserve unity; got {peak}"
    );
}

#[test]
fn e02_volume_block_disabled_acts_as_bypass() {
    let mut p = neutral_params("gain", "volume");
    p.insert("volume", ParameterValue::Float(0.0)); // would mute if enabled
    p.insert("mute", ParameterValue::Bool(true));
    let mut block = core_block("v", "gain", "volume", p);
    block.enabled = false;
    let (chain, registry) = chain_with_blocks(
        "e02",
        input_mono(vec![0]),
        vec![block],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.5], 2, 6);
    assert!(
        (peak - 0.5).abs() < TOLERANCE,
        "disabled volume block (mute=true) must bypass; got {peak}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// F. Stream lifecycle — fade-in completes; signal becomes steady
// ─────────────────────────────────────────────────────────────────────────

/// CONTRACT: after the FADE_IN_FRAMES (128) ramp at stream start, the
/// signal must reach unity gain steady. No perpetual ducking.
#[test]
fn f01_fade_in_completes_within_first_callback_at_buffer_256() {
    let (chain, registry) = chain_with_blocks(
        "f01",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let runtime = build_runtime(&chain, &registry);
    // First callback (256 frames) covers the 128-frame fade-in. Tail of
    // the buffer (frames 128..256) must already be at unity.
    let data = const_interleaved(&[0.5], 256);
    let out = drive_and_capture(&runtime, 1, &data, 2);
    // out is interleaved L,R; samples 256..512 correspond to frame 128+
    let tail = &out[256..];
    let tail_min = tail.iter().fold(f32::INFINITY, |a, &b| a.min(b.abs()));
    assert!(
        (tail_min - 0.5).abs() < TOLERANCE,
        "after fade-in (frames 128+), signal must be unity; got tail_min={tail_min}"
    );
}

#[test]
fn f02_fade_in_starts_at_zero_no_full_amplitude_burst() {
    let (chain, registry) = chain_with_blocks(
        "f02",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let runtime = build_runtime(&chain, &registry);
    let data = const_interleaved(&[0.5], 32);
    let out = drive_and_capture(&runtime, 1, &data, 2);
    // First 4 samples should still be ramping (gain near 0).
    let head_peak = peak_abs(&out[..8]);
    assert!(
        head_peak < 0.05,
        "fade-in head must start near zero gain; got peak {head_peak}"
    );
}

