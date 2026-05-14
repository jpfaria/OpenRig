//! Volume non-regression contract tests (issue #355, CLAUDE.md
//! invariant #10).
//!
//! THE RULE: nothing in this engine — refactor, fix, performance work,
//! cleanup, split — may alter per-stream volume without an explicit
//! user request. Solo guitar in any chain comes out at unity. Two
//! guitars summing to clipping is the output limiter's job, not a
//! preemptive 1/N scale. Mono passes through chains broadcasting
//! `Stereo([s, s])`. Stereo preserves `[L, R]`. Etc.
//!
//! These tests are the authoritative pin. If you break them, the
//! source is wrong, not the tests. Adjust the source until the tests
//! pass; never relax the assertions.
//!
//! Test groups:
//!
//!   A. Layout passthrough — every Input mode × Output mode combo
//!   B. Output limiter — transparent below 0.95, tanh above
//!   C. Volume block — unity / fractional gain
//!   D. Tremolo (user's actual culprit on Mac, 2026-04-28)
//!   E. Multi-block composition stays at unity when each is neutral
//!   F. Stream lifecycle (fade-in completes, then steady at unity)
//!   G. Split-mono (#350 / #355) — solo and dual cases
//!   H. Anti-revert structural pins
//!   J. User-reported reproducer

use super::{
    build_chain_runtime_state, process_input_f32, process_output_f32, AudioFrame,
    DEFAULT_ELASTIC_TARGET,
};
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::value_objects::ParameterValue;
use project::block::{
    schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry,
    OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::param::ParameterSet;
use std::sync::Arc;

const SR: f32 = 48_000.0;
const TOLERANCE: f32 = 1e-3;

// ─────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────

fn input_mono(channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId("input:0".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainInputMode::Mono,
                channels,
            }],
        }),
    }
}

fn input_stereo(channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId("input:0".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainInputMode::Stereo,
                channels,
            }],
        }),
    }
}

fn input_dual_mono(channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId("input:0".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainInputMode::DualMono,
                channels,
            }],
        }),
    }
}

fn output(mode: ChainOutputMode, channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId("output:0".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            entries: vec![OutputEntry {
                device_id: DeviceId("dev".into()),
                mode,
                channels,
            }],
        }),
    }
}

fn neutral_params(effect_type: &str, model: &str) -> ParameterSet {
    let schema =
        schema_for_block_model(effect_type, model).expect("schema must exist for test model");
    ParameterSet::default()
        .normalized_against(&schema)
        .expect("defaults must normalize")
}

fn core_block(id: &str, effect_type: &str, model: &str, params: ParameterSet) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: effect_type.to_string(),
            model: model.to_string(),
            params,
        }),
    }
}

fn chain_with_blocks(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some("test".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks,
    }
}

fn build_runtime(chain: &Chain) -> Arc<super::ChainRuntimeState> {
    Arc::new(
        build_chain_runtime_state(chain, SR, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime state should build"),
    )
}

fn drive_and_capture(
    runtime: &Arc<super::ChainRuntimeState>,
    input_total_channels: usize,
    data: &[f32],
    output_total_channels: usize,
) -> Vec<f32> {
    let num_frames = data.len() / input_total_channels;
    process_input_f32(runtime, 0, data, input_total_channels);
    let mut out = vec![0.0_f32; num_frames * output_total_channels];
    process_output_f32(runtime, 0, &mut out, output_total_channels);
    out
}

fn peak_abs(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |a, &b| a.max(b.abs()))
}

fn min_abs(samples: &[f32]) -> f32 {
    samples.iter().fold(f32::INFINITY, |a, &b| a.min(b.abs()))
}

fn const_interleaved(per_channel: &[f32], frames: usize) -> Vec<f32> {
    let mut buf = Vec::with_capacity(per_channel.len() * frames);
    for _ in 0..frames {
        for &v in per_channel {
            buf.push(v);
        }
    }
    buf
}

/// Run several callbacks; return the peak across the steady-state captures
/// (skip the first two callbacks to drop the FADE_IN ramp).
fn measure_steady_peak(
    chain: &Chain,
    input_channels: usize,
    per_channel: &[f32],
    output_channels: usize,
    callbacks: usize,
) -> f32 {
    let runtime = build_runtime(chain);
    let mut peaks: Vec<f32> = Vec::with_capacity(callbacks);
    for _ in 0..callbacks {
        let data = const_interleaved(per_channel, 256);
        let out = drive_and_capture(&runtime, input_channels, &data, output_channels);
        peaks.push(peak_abs(&out));
    }
    let steady = &peaks[2..];
    steady.iter().copied().fold(0.0_f32, |a, b| a.max(b))
}

/// Run several callbacks; return per-output-channel peak.
fn measure_steady_per_channel_peak(
    chain: &Chain,
    input_channels: usize,
    per_channel: &[f32],
    output_channels: usize,
    callbacks: usize,
) -> Vec<f32> {
    let runtime = build_runtime(chain);
    let mut last_out: Vec<f32> = Vec::new();
    for _ in 0..callbacks {
        let data = const_interleaved(per_channel, 256);
        last_out = drive_and_capture(&runtime, input_channels, &data, output_channels);
    }
    let mut per_ch_peak = vec![0.0_f32; output_channels];
    for (i, sample) in last_out.iter().enumerate() {
        let ch = i % output_channels;
        per_ch_peak[ch] = per_ch_peak[ch].max(sample.abs());
    }
    per_ch_peak
}

// ─────────────────────────────────────────────────────────────────────────
// A. Layout passthrough — every Input mode × Output mode combo
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn a01_mono_in_stereo_out_broadcasts_at_unity() {
    let chain = chain_with_blocks(
        "a01",
        vec![
            input_mono(vec![0]),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let peaks = measure_steady_per_channel_peak(&chain, 1, &[0.5], 2, 4);
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
    let chain = chain_with_blocks(
        "a02",
        vec![input_mono(vec![0]), output(ChainOutputMode::Mono, vec![0])],
    );
    let peak = measure_steady_peak(&chain, 1, &[0.4], 1, 4);
    assert!(
        (peak - 0.4).abs() < TOLERANCE,
        "mono in → mono out must be unity; got {peak}"
    );
}

#[test]
fn a03_stereo_in_stereo_out_preserves_l_and_r() {
    let chain = chain_with_blocks(
        "a03",
        vec![
            input_stereo(vec![0, 1]),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let peaks = measure_steady_per_channel_peak(&chain, 2, &[0.3, 0.6], 2, 4);
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
    let chain = chain_with_blocks(
        "a04",
        vec![
            input_dual_mono(vec![0, 1]),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let peaks = measure_steady_per_channel_peak(&chain, 2, &[0.25, 0.75], 2, 4);
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
    let chain = chain_with_blocks(
        "b01",
        vec![
            input_mono(vec![0]),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let peak = measure_steady_peak(&chain, 1, &[0.9], 2, 4);
    assert!(
        (peak - 0.9).abs() < TOLERANCE,
        "limiter must be transparent below 0.95; got {peak}"
    );
}

#[test]
fn b02_output_above_limiter_knee_applies_tanh() {
    // Send a hot input (1.5) through a passthrough chain. Mono → broadcast
    // Stereo([1.5, 1.5]) → write_output_frame applies tanh per channel.
    let chain = chain_with_blocks(
        "b02",
        vec![
            input_mono(vec![0]),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let peak = measure_steady_peak(&chain, 1, &[1.5], 2, 4);
    let expected = (1.5_f32).tanh();
    assert!(
        (peak - expected).abs() < 0.01,
        "above knee must equal tanh(sample); expected ≈ {expected}, got {peak}"
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
    let chain = chain_with_blocks(
        "c01",
        vec![
            input_mono(vec![0]),
            core_block("vol", "gain", "volume", params),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let peak = measure_steady_peak(&chain, 1, &[0.5], 2, 6);
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
    let chain = chain_with_blocks(
        "c02",
        vec![
            input_mono(vec![0]),
            core_block("vol", "gain", "volume", params),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let peak = measure_steady_peak(&chain, 1, &[0.5], 2, 6);
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
    let chain = chain_with_blocks(
        "c04",
        vec![
            input_mono(vec![0]),
            core_block("vol", "gain", "volume", params),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let peak = measure_steady_peak(&chain, 1, &[0.5], 2, 6);
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
    let chain = chain_with_blocks(
        "c03",
        vec![
            input_mono(vec![0]),
            core_block("vol", "gain", "volume", params),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let peak = measure_steady_peak(&chain, 1, &[0.5], 2, 6);
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
    let chain = chain_with_blocks(
        "d01",
        vec![
            input_mono(vec![0]),
            core_block("trem", "modulation", "tremolo_sine", params),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let peak = measure_steady_peak(&chain, 1, &[0.5], 2, 6);
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
    let chain = chain_with_blocks(
        "d02",
        vec![
            input_mono(vec![0]),
            core_block("trem", "modulation", "tremolo_sine", params),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    // Run many callbacks so the tremolo LFO traverses a full cycle.
    let runtime = build_runtime(&chain);
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
    let chain = chain_with_blocks(
        "d03",
        vec![
            input_mono(vec![0]),
            core_block("trem", "modulation", "tremolo_sine", params),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let runtime = build_runtime(&chain);
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
    let chain = chain_with_blocks(
        "e01",
        vec![
            input_mono(vec![0]),
            core_block("v1", "gain", "volume", p1),
            core_block("v2", "gain", "volume", p2),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let peak = measure_steady_peak(&chain, 1, &[0.5], 2, 6);
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
    let chain = chain_with_blocks(
        "e02",
        vec![
            input_mono(vec![0]),
            block,
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let peak = measure_steady_peak(&chain, 1, &[0.5], 2, 6);
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
    let chain = chain_with_blocks(
        "f01",
        vec![
            input_mono(vec![0]),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let runtime = build_runtime(&chain);
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
    let chain = chain_with_blocks(
        "f02",
        vec![
            input_mono(vec![0]),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let runtime = build_runtime(&chain);
    let data = const_interleaved(&[0.5], 32);
    let out = drive_and_capture(&runtime, 1, &data, 2);
    // First 4 samples should still be ramping (gain near 0).
    let head_peak = peak_abs(&out[..8]);
    assert!(
        head_peak < 0.05,
        "fade-in head must start near zero gain; got peak {head_peak}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// G. Split-mono (#350 / #355) — solo and dual cases
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn g01_split_mono_solo_emits_at_unity_gain() {
    let chain = chain_with_blocks(
        "g01",
        vec![
            input_mono(vec![0, 1]), // split-mono: 2 channels in mono mode
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let peak = measure_steady_peak(&chain, 2, &[0.5, 0.0], 2, 4);
    assert!(
        (peak - 0.5).abs() < TOLERANCE,
        "split-mono solo must emit at unity; got {peak}"
    );
}

#[test]
fn g02_split_mono_dual_below_limiter_knee_sums() {
    let chain = chain_with_blocks(
        "g02",
        vec![
            input_mono(vec![0, 1]),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let peak = measure_steady_peak(&chain, 2, &[0.3, 0.3], 2, 4);
    assert!(
        (peak - 0.6).abs() < TOLERANCE,
        "split-mono dual below knee must sum at unity per stream; got {peak}"
    );
}

#[test]
fn g03_split_mono_dual_above_knee_uses_tanh_limiter() {
    let chain = chain_with_blocks(
        "g03",
        vec![
            input_mono(vec![0, 1]),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let peak = measure_steady_peak(&chain, 2, &[0.8, 0.8], 2, 4);
    let expected = (1.6_f32).tanh();
    assert!(
        (peak - expected).abs() < 0.01,
        "split-mono dual above knee must equal tanh(sum); expected ≈ {expected}, got {peak}"
    );
}

#[test]
fn g04_mono_input_broadcasts_to_both_output_channels() {
    let chain = chain_with_blocks(
        "g04",
        vec![
            input_mono(vec![0]),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let peaks = measure_steady_per_channel_peak(&chain, 1, &[0.4], 2, 4);
    assert!(
        (peaks[0] - peaks[1]).abs() < TOLERANCE,
        "L peak {} must equal R peak {}",
        peaks[0],
        peaks[1]
    );
    assert!(
        peaks[0] > 0.3,
        "L must carry signal at unity; got {}",
        peaks[0]
    );
    assert!(
        peaks[1] > 0.3,
        "R must carry signal at unity; got {}",
        peaks[1]
    );
}

// ─────────────────────────────────────────────────────────────────────────
// H. Anti-revert structural pins
// ─────────────────────────────────────────────────────────────────────────

/// CONTRACT (CLAUDE.md invariant #10): split-mono solo must equal
/// single-mono solo at the same input level. A drift here means a
/// preemptive scale was reintroduced — search for `split_scale` in
/// runtime.rs and remove the attenuation.
#[test]
fn h01_split_mono_solo_equals_single_mono_solo() {
    let split = chain_with_blocks(
        "h01_split",
        vec![
            input_mono(vec![0, 1]),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let single = chain_with_blocks(
        "h01_single",
        vec![
            input_mono(vec![0]),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let split_peak = measure_steady_peak(&split, 2, &[0.5, 0.0], 2, 4);
    let single_peak = measure_steady_peak(&single, 1, &[0.5], 2, 4);
    assert!(
        (split_peak - single_peak).abs() < TOLERANCE,
        "split solo {split_peak} must equal single solo {single_peak} — \
         a drift means preemptive scaling was reintroduced"
    );
}

/// PIN: chain composition with a single pure-passthrough block (volume
/// at 100%) must preserve the same level as a chain WITHOUT that block.
/// Catches "block introduces hidden attenuation" silently.
#[test]
fn h02_neutral_block_addition_is_volume_preserving() {
    let bare = chain_with_blocks(
        "h02_bare",
        vec![
            input_mono(vec![0]),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let mut p = neutral_params("gain", "volume");
    p.insert("volume", ParameterValue::Float(100.0)); // unity point (issue #400 #3)
    p.insert("mute", ParameterValue::Bool(false));
    let with_block = chain_with_blocks(
        "h02_with",
        vec![
            input_mono(vec![0]),
            core_block("v", "gain", "volume", p),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let bare_peak = measure_steady_peak(&bare, 1, &[0.5], 2, 6);
    let with_peak = measure_steady_peak(&with_block, 1, &[0.5], 2, 6);
    assert!(
        (bare_peak - with_peak).abs() < 0.01,
        "neutral volume block must not change level; bare={bare_peak} with={with_peak}"
    );
}

/// PIN: mono → stereo bus broadcast must symmetric (L=R). Catches the
/// auto-pan regression of the original f38953a4 attempt at #350.
#[test]
fn h03_mono_to_stereo_bus_broadcast_is_symmetric() {
    let chain = chain_with_blocks(
        "h03",
        vec![
            input_mono(vec![0]),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let peaks = measure_steady_per_channel_peak(&chain, 1, &[0.6], 2, 4);
    assert!(
        (peaks[0] - peaks[1]).abs() < 1e-5,
        "L {} and R {} must be EXACTLY equal — broadcast is symmetric, no auto-pan",
        peaks[0],
        peaks[1]
    );
}

// ─────────────────────────────────────────────────────────────────────────
// J. User-reported reproducer (Mac, 2026-04-28)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn j01_user_reported_mac_volume_drop_does_not_recur() {
    let chain = chain_with_blocks(
        "j01",
        vec![
            input_mono(vec![0]),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let peak = measure_steady_peak(&chain, 1, &[0.3], 2, 8);
    assert!(
        (peak - 0.3).abs() < TOLERANCE,
        "Mac single-mono setup must hold steady at unity; got {peak}"
    );
}

/// REGRESSION DOC: replicates the user's CLEAN chain on 2026-04-28
/// EXACTLY (input mono [0] → blackface_clean with their params →
/// output stereo). Tremolo OMITTED — user clarified it's not active.
/// Filter omitted — disabled in their YAML. Only the amp.
///
/// Measures peak + RMS for a 0.4-amplitude 440 Hz sine input. The
/// numbers in the test output are the engine's authoritative answer
/// to "what does the engine produce for this exact input?". If the
/// user hears something quieter than the test reports, the
/// discrepancy is upstream of engine code (CoreAudio device gain,
/// Scarlett monitor knob, system output volume slider, headphone
/// gain on the Scarlett front panel).
#[test]
fn j02_user_clean_chain_blackface_only_signature() {
    let mut p = neutral_params("amp", "blackface_clean");
    p.insert("gain", ParameterValue::Float(0.0));
    p.insert("bass", ParameterValue::Float(50.0));
    p.insert("middle", ParameterValue::Float(50.0));
    p.insert("treble", ParameterValue::Float(50.0));
    p.insert("master", ParameterValue::Float(100.0));
    p.insert("output", ParameterValue::Float(50.0));
    p.insert("bright", ParameterValue::Bool(true));
    p.insert("sag", ParameterValue::Float(14.0));
    p.insert("room_mix", ParameterValue::Float(14.0));
    p.insert("input", ParameterValue::Float(50.0));
    let chain = chain_with_blocks(
        "j02",
        vec![
            input_mono(vec![0]),
            core_block("amp", "amp", "blackface_clean", p),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let runtime = build_runtime(&chain);
    let sr = SR;
    let mut all_samples: Vec<f32> = Vec::new();
    let mut phase = 0.0_f32;
    let inc = std::f32::consts::TAU * 440.0 / sr;
    for _ in 0..16 {
        let mut data = vec![0.0_f32; 256];
        for s in data.iter_mut() {
            *s = phase.sin() * 0.4;
            phase = (phase + inc).rem_euclid(std::f32::consts::TAU);
        }
        let out = drive_and_capture(&runtime, 1, &data, 2);
        all_samples.extend(out);
    }
    let steady = &all_samples[1024..];
    let peak = peak_abs(steady);
    let rms = (steady.iter().map(|s| s * s).sum::<f32>() / steady.len() as f32).sqrt();
    let peak_db = 20.0 * peak.log10();
    let rms_db = 20.0 * rms.log10();
    eprintln!(
        "[j02] blackface_clean signature: peak={peak} ({peak_db:.2} dBFS), \
         rms={rms} ({rms_db:.2} dBFS)"
    );
    assert!(
        peak > 0.5,
        "blackface_clean with master=100 + input 0.4 sine MUST output above 0.5 peak; \
         got {peak}. If this fails, engine is attenuating the signal — bug in code."
    );
    assert!(
        rms > 0.15,
        "blackface_clean RMS must be above 0.15; got {rms}. Low RMS = excessive limiter."
    );
}

#[allow(dead_code)]
fn _suppress_audio_frame_dead_code(f: AudioFrame) -> AudioFrame {
    f
}

// ─────────────────────────────────────────────────────────────────────────
// K. preset.volume (issue #440)
// ─────────────────────────────────────────────────────────────────────────
//
// Chain.volume é aplicado pelo engine no master output do
// process_output_f32. Estes tests verificam:
//   1. build_chain_runtime_state lê o chain.volume e seta o atomic.
//   2. update_chain_runtime_state (chain edit) propaga o volume novo.
//   3. process_output_f32 multiplica out pelo volume / 100.
//
// Sem esses tests, o handler Slint pode acionar o callback Rust e o
// usuário não ouve diferença porque o engine não está propagando.

const VOLUME_TOLERANCE: f32 = 0.01;

fn unity_passthrough_chain(id: &str, volume: f32) -> Chain {
    let mut chain = chain_with_blocks(
        id,
        vec![
            input_mono(vec![0]),
            output(ChainOutputMode::Mono, vec![0]),
        ],
    );
    chain.volume = volume;
    chain
}

#[test]
fn k01_chain_volume_100_is_unity() {
    let chain = unity_passthrough_chain("k01", 100.0);
    let runtime = build_runtime(&chain);
    assert!(
        (runtime.volume_pct() - 100.0).abs() < VOLUME_TOLERANCE,
        "build_chain_runtime_state should propagate chain.volume=100 \
         to runtime.volume_pct(); got {}",
        runtime.volume_pct()
    );
    let peak = measure_steady_peak(&chain, 1, &[0.5], 1, 5);
    assert!(
        (peak - 0.5).abs() < VOLUME_TOLERANCE,
        "volume=100 should be unity; expected peak≈0.5, got {peak}"
    );
}

#[test]
fn k02_chain_volume_50_halves_output() {
    let chain = unity_passthrough_chain("k02", 50.0);
    let runtime = build_runtime(&chain);
    assert!(
        (runtime.volume_pct() - 50.0).abs() < VOLUME_TOLERANCE,
        "chain.volume=50 should land on runtime.volume_pct(); got {}",
        runtime.volume_pct()
    );
    let peak = measure_steady_peak(&chain, 1, &[0.5], 1, 5);
    assert!(
        (peak - 0.25).abs() < VOLUME_TOLERANCE,
        "volume=50 attenuates by half; expected peak≈0.25, got {peak}"
    );
}

#[test]
fn k03_chain_volume_200_doubles_output() {
    let chain = unity_passthrough_chain("k03", 200.0);
    let peak = measure_steady_peak(&chain, 1, &[0.3], 1, 5);
    // input 0.3 × 2.0 = 0.6
    assert!(
        (peak - 0.6).abs() < VOLUME_TOLERANCE,
        "volume=200 doubles; expected peak≈0.6, got {peak}"
    );
}

#[test]
fn k04_chain_volume_zero_silences_output() {
    let chain = unity_passthrough_chain("k04", 0.0);
    let peak = measure_steady_peak(&chain, 1, &[0.5], 1, 5);
    assert!(
        peak < VOLUME_TOLERANCE,
        "volume=0 should silence output; got peak {peak}"
    );
}

#[test]
fn k05_update_chain_runtime_state_propagates_volume() {
    // Cenário do bug que o usuário reportou: chain construída com
    // volume=100, slider arrasta pra 150, engine deve VER 150 sem
    // teardown. update_chain_runtime_state é o path que o handler
    // chain_volume_changed dispara via sync_live_chain_runtime → upsert
    // → update_chain_runtime_state.
    let chain100 = unity_passthrough_chain("k05", 100.0);
    let runtime = build_runtime(&chain100);
    assert!((runtime.volume_pct() - 100.0).abs() < VOLUME_TOLERANCE);

    let mut chain150 = chain100.clone();
    chain150.volume = 150.0;
    super::update_chain_runtime_state(
        &runtime,
        &chain150,
        SR,
        false,
        &[DEFAULT_ELASTIC_TARGET],
    )
    .expect("update_chain_runtime_state should propagate volume");
    assert!(
        (runtime.volume_pct() - 150.0).abs() < VOLUME_TOLERANCE,
        "after update_chain_runtime_state with chain.volume=150, \
         runtime.volume_pct() should be 150; got {}",
        runtime.volume_pct()
    );

    // Sanity: process_output_f32 reflete o novo volume sem rebuild.
    let data = const_interleaved(&[0.4], 256);
    // Drain initial fade-in callbacks.
    for _ in 0..3 {
        let _ = drive_and_capture(&runtime, 1, &data, 1);
    }
    let out = drive_and_capture(&runtime, 1, &data, 1);
    let peak = peak_abs(&out);
    // input 0.4 × 1.5 = 0.6
    assert!(
        (peak - 0.6).abs() < VOLUME_TOLERANCE,
        "after update to volume=150, peak should be ≈0.6 (0.4 × 1.5); got {peak}"
    );
}
