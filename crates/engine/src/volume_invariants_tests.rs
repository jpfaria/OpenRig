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
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use std::sync::Arc;

const SR: f32 = 48_000.0;
const TOLERANCE: f32 = 1e-3;

// ─────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────

/// Registry id every chain in this file references via `io_binding_ids`.
const IO_BINDING_ID: &str = "io";

// The chain's physical I/O lives in the per-machine registry now (model A).
// These helpers return the registry endpoint describing one input / output;
// device / mode / channels are preserved exactly from the old Input/Output
// blocks — only the SET-UP form changed.

fn input_mono(channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: "in0".into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels,
    }
}

fn input_stereo(channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: "in0".into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels,
    }
}

fn input_dual_mono(channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: "in0".into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::DualMono,
        channels,
    }
}

fn output(mode: ChannelMode, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: "out0".into(),
        device_id: DeviceId("dev".into()),
        mode,
        channels,
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

fn chain_with_blocks(
    id: &str,
    input_ep: IoEndpoint,
    fx: Vec<AudioBlock>,
    output_ep: IoEndpoint,
) -> (Chain, Vec<IoBinding>) {
    let registry = vec![IoBinding {
        id: IO_BINDING_ID.into(),
        name: "IO".into(),
        inputs: vec![input_ep],
        outputs: vec![output_ep],
    }];
    let chain = Chain {
        id: ChainId(id.into()),
        description: Some("test".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![IO_BINDING_ID.into()],
        blocks: fx,
    };
    (chain, registry)
}

fn build_runtime(chain: &Chain, registry: &[IoBinding]) -> Arc<super::ChainRuntimeState> {
    Arc::new(
        build_chain_runtime_state(chain, SR, &[DEFAULT_ELASTIC_TARGET], registry)
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
    registry: &[IoBinding],
    input_channels: usize,
    per_channel: &[f32],
    output_channels: usize,
    callbacks: usize,
) -> f32 {
    let runtime = build_runtime(chain, registry);
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
    registry: &[IoBinding],
    input_channels: usize,
    per_channel: &[f32],
    output_channels: usize,
    callbacks: usize,
) -> Vec<f32> {
    let runtime = build_runtime(chain, registry);
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
        vec![core_block("v1", "gain", "volume", p1), core_block("v2", "gain", "volume", p2)],
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

// ─────────────────────────────────────────────────────────────────────────
// G. Split-mono (#350 / #355) — solo and dual cases
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn g01_split_mono_solo_emits_at_unity_gain() {
    let (chain, registry) = chain_with_blocks(
        "g01",
        input_mono(vec![0, 1]),
        vec![],
        // split-mono: 2 channels in mono mode
            output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peak = measure_steady_peak(&chain, &registry, 2, &[0.5, 0.0], 2, 4);
    assert!(
        (peak - 0.5).abs() < TOLERANCE,
        "split-mono solo must emit at unity; got {peak}"
    );
}

#[test]
fn g02_split_mono_dual_below_limiter_knee_sums() {
    let (chain, registry) = chain_with_blocks(
        "g02",
        input_mono(vec![0, 1]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peak = measure_steady_peak(&chain, &registry, 2, &[0.3, 0.3], 2, 4);
    assert!(
        (peak - 0.6).abs() < TOLERANCE,
        "split-mono dual below knee must sum at unity per stream; got {peak}"
    );
}

#[test]
fn g03_split_mono_dual_above_knee_is_softly_saturated() {
    // Issue #496: pin the PROPERTIES instead of `peak ≈ tanh(sum)`.
    // The old tanh form was discontinuous + non-monotonic (RED in
    // runtime_dsp::tests). What this invariant really guards is "when
    // dual mono sums above the knee, the output stays bounded and
    // loud — no DAC clip, no quiet collapse".
    let (chain, registry) = chain_with_blocks(
        "g03",
        input_mono(vec![0, 1]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peak = measure_steady_peak(&chain, &registry, 2, &[0.8, 0.8], 2, 4);
    assert!(
        peak <= 1.0,
        "split-mono dual sum must be bounded ≤ 1.0; got {peak}"
    );
    assert!(peak < 1.6, "must be reduced from raw sum 1.6; got {peak}");
    assert!(peak > 0.9, "must stay loud (no quiet collapse); got {peak}");
}

#[test]
fn g04_mono_input_broadcasts_to_both_output_channels() {
    let (chain, registry) = chain_with_blocks(
        "g04",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peaks = measure_steady_per_channel_peak(&chain, &registry, 1, &[0.4], 2, 4);
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
    let (split, split_registry) = chain_with_blocks(
        "h01_split",
        input_mono(vec![0, 1]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let (single, single_registry) = chain_with_blocks(
        "h01_single",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let split_peak = measure_steady_peak(&split, &split_registry, 2, &[0.5, 0.0], 2, 4);
    let single_peak = measure_steady_peak(&single, &single_registry, 1, &[0.5], 2, 4);
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
    let (bare, bare_registry) = chain_with_blocks(
        "h02_bare",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let mut p = neutral_params("gain", "volume");
    p.insert("volume", ParameterValue::Float(100.0)); // unity point (issue #400 #3)
    p.insert("mute", ParameterValue::Bool(false));
    let (with_block, with_registry) = chain_with_blocks(
        "h02_with",
        input_mono(vec![0]),
        vec![core_block("v", "gain", "volume", p)],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let bare_peak = measure_steady_peak(&bare, &bare_registry, 1, &[0.5], 2, 6);
    let with_peak = measure_steady_peak(&with_block, &with_registry, 1, &[0.5], 2, 6);
    assert!(
        (bare_peak - with_peak).abs() < 0.01,
        "neutral volume block must not change level; bare={bare_peak} with={with_peak}"
    );
}

/// PIN: mono → stereo bus broadcast must symmetric (L=R). Catches the
/// auto-pan regression of the original f38953a4 attempt at #350.
#[test]
fn h03_mono_to_stereo_bus_broadcast_is_symmetric() {
    let (chain, registry) = chain_with_blocks(
        "h03",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peaks = measure_steady_per_channel_peak(&chain, &registry, 1, &[0.6], 2, 4);
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
    let (chain, registry) = chain_with_blocks(
        "j01",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.3], 2, 8);
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
    let (chain, registry) = chain_with_blocks(
        "j02",
        input_mono(vec![0]),
        vec![core_block("amp", "amp", "blackface_clean", p)],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let runtime = build_runtime(&chain, &registry);
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

fn unity_passthrough_chain(id: &str, volume: f32) -> (Chain, Vec<IoBinding>) {
    let (mut chain, registry) = chain_with_blocks(
        id,
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Mono, vec![0]),
    );
    chain.volume = volume;
    (chain, registry)
}

#[test]
fn k01_chain_volume_100_is_unity() {
    let (chain, registry) = unity_passthrough_chain("k01", 100.0);
    let runtime = build_runtime(&chain, &registry);
    assert!(
        (runtime.volume_pct() - 100.0).abs() < VOLUME_TOLERANCE,
        "build_chain_runtime_state should propagate chain.volume=100 \
         to runtime.volume_pct(); got {}",
        runtime.volume_pct()
    );
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.5], 1, 5);
    assert!(
        (peak - 0.5).abs() < VOLUME_TOLERANCE,
        "volume=100 should be unity; expected peak≈0.5, got {peak}"
    );
}

#[test]
fn k02_chain_volume_50_halves_output() {
    let (chain, registry) = unity_passthrough_chain("k02", 50.0);
    let runtime = build_runtime(&chain, &registry);
    assert!(
        (runtime.volume_pct() - 50.0).abs() < VOLUME_TOLERANCE,
        "chain.volume=50 should land on runtime.volume_pct(); got {}",
        runtime.volume_pct()
    );
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.5], 1, 5);
    assert!(
        (peak - 0.25).abs() < VOLUME_TOLERANCE,
        "volume=50 attenuates by half; expected peak≈0.25, got {peak}"
    );
}

#[test]
fn k03_chain_volume_200_doubles_output() {
    let (chain, registry) = unity_passthrough_chain("k03", 200.0);
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.3], 1, 5);
    // input 0.3 × 2.0 = 0.6
    assert!(
        (peak - 0.6).abs() < VOLUME_TOLERANCE,
        "volume=200 doubles; expected peak≈0.6, got {peak}"
    );
}

#[test]
fn k07_volume_boost_on_hot_signal_is_limited_not_clipped() {
    // The user's CPM 22 case: Chain.volume = 145 on a hot chain. Today the
    // master volume is multiplied AFTER the per-sample output limiter, so a
    // hot signal (≈0.9) limited to ≈0.72 then ×2.0 = 1.43 — hard clip at the
    // DAC, NOTHING limits after the multiply on the single-stream path.
    //
    // Contract (this file's header: "clipping is the output limiter's job"):
    // volume must be applied BEFORE the limiter so the limiter sees the
    // post-volume signal and holds it ≤ full scale. With the fix:
    //   0.9 × 2.0 = 1.8 → tanh(1.8) ≈ 0.947  (clip-free)
    // The k01–k04 invariants use sub-0.95 signals so they are unaffected
    // either way (tanh transparent below the knee).
    let (chain, registry) = unity_passthrough_chain("k07", 200.0);
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.9], 1, 5);
    assert!(
        peak <= 1.0 + VOLUME_TOLERANCE,
        "hot signal × volume boost must be limited ≤ full scale, not hard \
         clipped; got peak {peak} (volume applied after the limiter = bug)"
    );
}

#[test]
fn k04_chain_volume_zero_silences_output() {
    let (chain, registry) = unity_passthrough_chain("k04", 0.0);
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.5], 1, 5);
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
    let (chain100, registry) = unity_passthrough_chain("k05", 100.0);
    let runtime = build_runtime(&chain100, &registry);
    assert!((runtime.volume_pct() - 100.0).abs() < VOLUME_TOLERANCE);

    let mut chain150 = chain100.clone();
    chain150.volume = 150.0;
    super::update_chain_runtime_state(&runtime, &chain150, SR, false, &[DEFAULT_ELASTIC_TARGET], &registry)
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

#[test]
fn k06_runtime_graph_upsert_propagates_volume_on_existing_chain() {
    // Reproduz exatamente o caminho que o slider dispara em produção:
    // chain_row_wiring::on_chain_volume_changed → sync_live_chain_runtime →
    // ProjectRuntimeController::upsert_chain → upsert_chain_with_resolved →
    // RuntimeGraph::upsert_chain (chain já existe → update_chain_runtime_state).
    //
    // Se este test passar mas o app não responder ao slider, o bug está
    // FORA do engine (Slint callback, Rust handler, ou outra camada).
    let (chain_v100, registry) = unity_passthrough_chain("k06", 100.0);
    let mut graph = crate::runtime_graph::RuntimeGraph {
        chains: std::collections::HashMap::new(),
    };

    let runtime = graph
        .upsert_chain(&chain_v100, SR, false, &[DEFAULT_ELASTIC_TARGET], &registry)
        .expect("first upsert builds chain runtime");
    assert!(
        (runtime.volume_pct() - 100.0).abs() < VOLUME_TOLERANCE,
        "first upsert: volume_pct should be 100; got {}",
        runtime.volume_pct()
    );

    // Slider arrasta de 100 pra 175. Handler atualiza chain.volume e
    // re-upserta no graph. Como a chain já existe, vai pro path de
    // update_chain_runtime_state — DEVE refletir sem teardown.
    let mut chain_v175 = chain_v100.clone();
    chain_v175.volume = 175.0;
    let runtime_after = graph
        .upsert_chain(&chain_v175, SR, false, &[DEFAULT_ELASTIC_TARGET], &registry)
        .expect("re-upsert updates volume in place");
    assert!(
        Arc::ptr_eq(&runtime, &runtime_after),
        "re-upsert with existing chain should return the SAME Arc, \
         confirming update_chain_runtime_state ran (not rebuild)"
    );
    assert!(
        (runtime_after.volume_pct() - 175.0).abs() < VOLUME_TOLERANCE,
        "after re-upsert with chain.volume=175, runtime.volume_pct() \
         should be 175; got {}",
        runtime_after.volume_pct()
    );
}

// ─────────────────────────────────────────────────────────────────────────
// L. Real-engine spectral / quality audit (issue #496).
//
// Drives PINK NOISE (= equal energy per octave, the universal frequency-
// response reference) through a *real* OpenRig chain — chain → runtime
// → `process_input_f32` → `process_output_f32` — and measures objective
// quality on what comes out. No ear, no synthetic math substitute. If
// the bare path (input + output, no blocks) colours the spectrum or
// adds noise, "all-native chain sounds broken" is caught here.
// ─────────────────────────────────────────────────────────────────────────

fn pink_noise(n: usize, seed: u64) -> Vec<f32> {
    use std::num::Wrapping;
    let mut state = Wrapping(seed);
    let mut rng = || {
        state = state * Wrapping(6364136223846793005) + Wrapping(1442695040888963407);
        ((state.0 >> 33) as f32 / u32::MAX as f32) * 2.0 - 1.0
    };
    const ROWS: usize = 16;
    let mut rows = [0.0f32; ROWS];
    let mut last_total = 0.0f32;
    (0..n)
        .map(|i| {
            let mut idx = 0;
            let mut k = i;
            while k & 1 == 0 && idx < ROWS - 1 {
                k >>= 1;
                idx += 1;
            }
            let new = rng();
            let total = last_total - rows[idx] + new;
            rows[idx] = new;
            last_total = total;
            (total / (ROWS as f32 * 0.6)).clamp(-0.7, 0.7)
        })
        .collect()
}

fn fft_octave_db(samples: &[f32], sr: f32) -> Vec<(f32, f32)> {
    use rustfft::{num_complex::Complex, FftPlanner};
    let n = samples.len().next_power_of_two();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let mut buf: Vec<Complex<f32>> = samples
        .iter()
        .map(|&s| Complex::new(s, 0.0))
        .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
        .take(n)
        .collect();
    fft.process(&mut buf);
    let bin_hz = sr / n as f32;
    let centres = [
        62.5_f32, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0,
    ];
    centres
        .iter()
        .map(|&fc| {
            let lo_b = ((fc / std::f32::consts::SQRT_2) / bin_hz).floor() as usize;
            let hi_b = (((fc * std::f32::consts::SQRT_2) / bin_hz).ceil() as usize).min(n / 2);
            let energy: f32 = buf[lo_b..hi_b].iter().map(|c| c.norm_sqr()).sum();
            (fc, 10.0 * energy.max(1e-12).log10())
        })
        .collect()
}

/// Drive `samples` through the real engine, return the captured output
/// as a single mono-equivalent stream (sum of stereo channels if any).
fn run_pink_through_chain(chain: &Chain, registry: &[IoBinding], mono_samples: &[f32]) -> Vec<f32> {
    let runtime = build_runtime(chain, registry);
    let buffer = 512usize;
    let n_callbacks = mono_samples.len().div_ceil(buffer);
    let mut out_collected: Vec<f32> = Vec::with_capacity(mono_samples.len());
    for cb in 0..n_callbacks {
        let start = cb * buffer;
        let end = (start + buffer).min(mono_samples.len());
        let chunk = &mono_samples[start..end];
        process_input_f32(&runtime, 0, chunk, 1);
        let mut out = vec![0.0_f32; chunk.len() * 2]; // assume stereo out
        process_output_f32(&runtime, 0, &mut out, 2);
        for f in out.chunks_exact(2) {
            out_collected.push((f[0] + f[1]) * 0.5);
        }
    }
    out_collected
}

#[test]
fn l01_real_engine_bare_chain_preserves_spectrum_per_octave() {
    // The simplest possible REAL chain: mono input → stereo output,
    // no blocks. If even THIS colours the spectrum, every chain is
    // mangled at the I/O layer — that's the structural bug.
    let (chain, registry) = chain_with_blocks(
        "l01",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let pink = pink_noise(SR as usize * 2, 0xC0FFEE);
    let out = run_pink_through_chain(&chain, &registry, &pink);
    // Skip the fade-in tail (first ~50 ms of warmup callbacks).
    let skip = (SR as usize) / 20;
    let in_bands = fft_octave_db(&pink[skip..], SR);
    let out_bands = fft_octave_db(&out[skip..], SR);
    eprintln!("\n=== REAL engine bare chain @ unity (mono→stereo) ===");
    eprintln!(" centre Hz   in dB    out dB    delta");
    let mut worst = (0.0_f32, 0.0_f32);
    for ((fc, i), (_, o)) in in_bands.iter().zip(out_bands.iter()) {
        let d = o - i;
        eprintln!(" {fc:>9.1}   {i:>7.2}   {o:>7.2}   {d:>+6.2}");
        if d.abs() > worst.1.abs() {
            worst = (*fc, d);
        }
    }
    assert!(
        worst.1.abs() < 1.0,
        "REAL ENGINE coloured the spectrum at {} Hz by {:+.2} dB — \
         every chain is bandpassed by the bare path",
        worst.0,
        worst.1
    );
}

#[test]
fn l02_real_engine_bare_chain_thd_n_low_for_pure_sine() {
    use rustfft::{num_complex::Complex, FftPlanner};
    let (chain, registry) = chain_with_blocks(
        "l02",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let n: usize = SR as usize;
    let sig: Vec<f32> = (0..n)
        .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin())
        .collect();
    let out = run_pink_through_chain(&chain, &registry, &sig);
    let skip = (SR as usize) / 20;
    // Issue #496 measurement fix: integer cycles, no zero-pad.
    let cycle_samples = (SR / 1_000.0).round() as usize;
    let usable = ((out.len() - skip) / cycle_samples) * cycle_samples;
    let tail = &out[skip..skip + usable];
    let nfft = tail.len();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(nfft);
    let mut buf: Vec<Complex<f32>> = tail.iter().map(|&s| Complex::new(s, 0.0)).collect();
    fft.process(&mut buf);
    let bin_hz = SR / nfft as f32;
    let fb = (1_000.0 / bin_hz).round() as usize;
    let fundamental: f32 = (fb.saturating_sub(1)..=fb + 1)
        .map(|b| buf[b].norm_sqr())
        .sum();
    let total: f32 = buf[..nfft / 2].iter().map(|c| c.norm_sqr()).sum();
    let thd_n_db = 10.0 * ((total - fundamental).max(1e-12) / fundamental).log10();
    eprintln!("\n=== REAL engine THD+N @ 1 kHz mono→stereo ===\n  THD+N = {thd_n_db:.2} dB");
    assert!(thd_n_db < -60.0, "THD+N = {thd_n_db:.2} dB");
}

#[test]
fn l03_real_engine_bare_chain_lufs_transparent_at_unity() {
    use ebur128::{EbuR128, Mode};
    let (chain, registry) = chain_with_blocks(
        "l03",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let pink = pink_noise(SR as usize * 3, 0xBADA55);
    let out = run_pink_through_chain(&chain, &registry, &pink);
    let skip = (SR as usize) / 20;
    let mut m_in = EbuR128::new(1, SR as u32, Mode::I).unwrap();
    m_in.add_frames_f32(&pink[skip..]).unwrap();
    let mut m_out = EbuR128::new(1, SR as u32, Mode::I).unwrap();
    m_out.add_frames_f32(&out[skip..]).unwrap();
    let lin = m_in.loudness_global().unwrap();
    let lout = m_out.loudness_global().unwrap();
    eprintln!(
        "\n=== REAL engine bare chain LUFS @ unity ===\n  in  = {lin:>7.2} LUFS\n  out = {lout:>7.2} LUFS\n  delta = {:+.2} dB",
        lout - lin
    );
    assert!(
        (lout - lin).abs() < 1.0,
        "REAL ENGINE bare chain LUFS delta {:.2} dB — should be transparent",
        lout - lin
    );
}

/// Drive a long signal and report THD+N AFTER a generous skip — kills
/// the fade-in hypothesis. If THD+N is still bad with skip = 1 s, the
/// noise is steady-state from the path, not a startup transient.
#[test]
fn l04_real_engine_thd_after_one_second_skip_isolates_fade_in() {
    use rustfft::{num_complex::Complex, FftPlanner};
    let (chain, registry) = chain_with_blocks(
        "l04",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let n: usize = (SR as usize) * 3; // 3 seconds
    let sig: Vec<f32> = (0..n)
        .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin())
        .collect();
    let out = run_pink_through_chain(&chain, &registry, &sig);
    let skip = SR as usize; // skip first 1 s
                            // Issue #496 measurement fix: integer cycles, no zero-pad.
    let cycle_samples = (SR / 1_000.0).round() as usize;
    let usable = ((out.len() - skip) / cycle_samples) * cycle_samples;
    let tail = &out[skip..skip + usable];
    let nfft = tail.len();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(nfft);
    let mut buf: Vec<Complex<f32>> = tail.iter().map(|&s| Complex::new(s, 0.0)).collect();
    fft.process(&mut buf);
    let bin_hz = SR / nfft as f32;
    let fb = (1_000.0 / bin_hz).round() as usize;
    let fundamental: f32 = (fb.saturating_sub(1)..=fb + 1)
        .map(|b| buf[b].norm_sqr())
        .sum();
    let total: f32 = buf[..nfft / 2].iter().map(|c| c.norm_sqr()).sum();
    let thd_n_db = 10.0 * ((total - fundamental).max(1e-12) / fundamental).log10();
    eprintln!("\n=== L04 THD+N (3s sine, 1s skip) ===\n  THD+N = {thd_n_db:.2} dB");
    assert!(
        thd_n_db < -60.0,
        "L04: THD+N {thd_n_db:.2} dB after 1s skip"
    );
}

/// Drive SILENCE and capture output. A clean path produces pure
/// zeros. Any non-zero sample = engine is injecting (fade-in tail,
/// underrun, buffer state, anything).
#[test]
fn l05_real_engine_silent_input_must_produce_silent_output() {
    let (chain, registry) = chain_with_blocks(
        "l05",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let sig = vec![0.0_f32; (SR as usize) * 2];
    let out = run_pink_through_chain(&chain, &registry, &sig);
    let skip = SR as usize; // 1 s skip
    let tail = &out[skip..];
    let peak = tail.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
    let rms = (tail.iter().map(|v| v * v).sum::<f32>() / tail.len() as f32).sqrt();
    eprintln!("\n=== L05 silent input ===\n  peak = {peak:.6}  rms = {rms:.6}");
    assert!(
        peak < 1e-6,
        "L05: silent input produced non-silent output: peak {peak:.6}"
    );
}

/// DC input (a constant) — there is no signal to harmonise, so any
/// AC content in the output is path-injected noise. Pure isolator.
#[test]
fn l06_real_engine_dc_input_steady_output_has_no_ac_noise() {
    let (chain, registry) = chain_with_blocks(
        "l06",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let sig = vec![0.3_f32; (SR as usize) * 2];
    let out = run_pink_through_chain(&chain, &registry, &sig);
    let skip = SR as usize;
    let tail = &out[skip..];
    let mean = tail.iter().sum::<f32>() / tail.len() as f32;
    let ac_rms = (tail.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / tail.len() as f32).sqrt();
    eprintln!(
        "\n=== L06 DC input (0.3 const) ===\n  output mean = {mean:.6}  AC rms = {ac_rms:.6e}"
    );
    assert!(
        ac_rms < 5e-4,
        "L06: DC in → AC noise out (rms {ac_rms:.6e}, > -66 dBFS = audible)"
    );
}

/// Mono input broadcasts to BOTH stereo output channels — they must
/// be byte-identical. If they drift, the broadcast itself has a bug.
#[test]
fn l07_real_engine_mono_broadcast_writes_identical_l_and_r() {
    let (chain, registry) = chain_with_blocks(
        "l07",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let runtime = build_runtime(&chain, &registry);
    let n_frames = 512;
    let sig: Vec<f32> = (0..n_frames)
        .map(|i| 0.4 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / SR).sin())
        .collect();
    // Drive a few callbacks then capture the steady one.
    for _ in 0..6 {
        process_input_f32(&runtime, 0, &sig, 1);
    }
    let mut out = vec![0.0_f32; n_frames * 2];
    process_output_f32(&runtime, 0, &mut out, 2);
    let mut max_drift = 0.0_f32;
    for f in out.chunks_exact(2) {
        let drift = (f[0] - f[1]).abs();
        if drift > max_drift {
            max_drift = drift;
        }
    }
    eprintln!("\n=== L07 mono→stereo broadcast ===\n  max L vs R drift = {max_drift:.6}");
    assert!(
        max_drift < 1e-6,
        "L07: broadcast L and R drift by {max_drift:.6} — broadcast is BROKEN"
    );
}

/// Run the SAME signal through TWO different callback buffer sizes
/// and check the output is the same. A path that depends on buffer
/// size has state leaking somewhere (elastic buffer, fade-in counter,
/// FIFO underflow). Same input ⇒ same output.
#[test]
fn l08_real_engine_thd_is_independent_of_callback_buffer_size() {
    use rustfft::{num_complex::Complex, FftPlanner};
    let (chain, registry) = chain_with_blocks(
        "l08",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let n: usize = (SR as usize) * 2;
    let sig: Vec<f32> = (0..n)
        .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin())
        .collect();

    let drive = |buffer: usize| -> f32 {
        let target = DEFAULT_ELASTIC_TARGET.max(buffer);
        let runtime = Arc::new(build_chain_runtime_state(&chain, SR, &[target], &registry).expect("runtime"));
        let mut out_collected: Vec<f32> = Vec::with_capacity(sig.len());
        for chunk in sig.chunks(buffer) {
            process_input_f32(&runtime, 0, chunk, 1);
            let mut out = vec![0.0_f32; chunk.len() * 2];
            process_output_f32(&runtime, 0, &mut out, 2);
            for f in out.chunks_exact(2) {
                out_collected.push((f[0] + f[1]) * 0.5);
            }
        }
        let skip = SR as usize;
        // Issue #496 measurement fix: integer cycles, no zero-pad.
        let cycle_samples = (SR / 1_000.0).round() as usize;
        let usable = ((out_collected.len() - skip) / cycle_samples) * cycle_samples;
        let tail = &out_collected[skip..skip + usable];
        let nfft = tail.len();
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(nfft);
        let mut buf: Vec<Complex<f32>> = tail.iter().map(|&s| Complex::new(s, 0.0)).collect();
        fft.process(&mut buf);
        let bin_hz = SR / nfft as f32;
        let fb = (1_000.0 / bin_hz).round() as usize;
        let fundamental: f32 = (fb.saturating_sub(1)..=fb + 1)
            .map(|b| buf[b].norm_sqr())
            .sum();
        let total: f32 = buf[..nfft / 2].iter().map(|c| c.norm_sqr()).sum();
        10.0 * ((total - fundamental).max(1e-12) / fundamental).log10()
    };

    let thd_128 = drive(128);
    let thd_512 = drive(512);
    let thd_2048 = drive(2048);
    eprintln!(
        "\n=== L08 THD+N vs buffer size ===\n  128 frames  → {thd_128:.2} dB\n  512 frames  → {thd_512:.2} dB\n  2048 frames → {thd_2048:.2} dB"
    );
    let spread = thd_128.max(thd_512.max(thd_2048)) - thd_128.min(thd_512.min(thd_2048));
    assert!(
        spread < 3.0,
        "L08: THD+N depends on buffer size (spread = {spread:.2} dB) — elastic / FIFO bug"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// M. Elastic-buffer / SPSC-ring path audit (issue #496, target found via
//    L08). 30+ RED tests probing the exact culprit: per-callback buffer
//    sizes, signal levels, frequencies, DC, silence, LUFS — each test
//    isolates one independent property of a clean signal path.
// ─────────────────────────────────────────────────────────────────────────

fn thd_n_db_through_chain(chain: &Chain, registry: &[IoBinding], sig: &[f32], buffer: usize) -> f32 {
    thd_n_db_at_freq_through_chain(chain, registry, sig, buffer, 1_000.0)
}

fn thd_n_db_at_freq_through_chain(chain: &Chain, registry: &[IoBinding], sig: &[f32], buffer: usize, freq: f32) -> f32 {
    use rustfft::{num_complex::Complex, FftPlanner};
    let target = DEFAULT_ELASTIC_TARGET.max(buffer);
    let runtime = Arc::new(build_chain_runtime_state(chain, SR, &[target], registry).expect("runtime"));
    let mut out_collected: Vec<f32> = Vec::with_capacity(sig.len());
    for chunk in sig.chunks(buffer) {
        process_input_f32(&runtime, 0, chunk, 1);
        let mut out = vec![0.0_f32; chunk.len() * 2];
        process_output_f32(&runtime, 0, &mut out, 2);
        for f in out.chunks_exact(2) {
            out_collected.push((f[0] + f[1]) * 0.5);
        }
    }
    // Issue #496 measurement-bug fix: truncate the tail to an exact
    // integer number of fundamental cycles BEFORE the FFT. Zero-padding
    // a non-periodic window injects spectral leakage that an earlier
    // version of this helper counted as engine-side noise, producing
    // false THD+N values of -13 dB on a path that is in fact bit-exact
    // after fade-in (verified by `diag_multi_callback_bit_exact_*`).
    let skip = SR as usize;
    let cycle_samples = (SR / freq).round().max(1.0) as usize;
    let usable_total = out_collected.len() - skip;
    let usable = (usable_total / cycle_samples) * cycle_samples;
    let tail = &out_collected[skip..skip + usable];
    let nfft = tail.len();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(nfft);
    let mut buf: Vec<Complex<f32>> = tail.iter().map(|&s| Complex::new(s, 0.0)).collect();
    fft.process(&mut buf);
    let bin_hz = SR / nfft as f32;
    let fb = (freq / bin_hz).round() as usize;
    let fundamental: f32 = (fb.saturating_sub(1)..=fb + 1)
        .map(|b| buf[b].norm_sqr())
        .sum();
    let total: f32 = buf[..nfft / 2].iter().map(|c| c.norm_sqr()).sum();
    10.0 * ((total - fundamental).max(1e-12) / fundamental).log10()
}

fn ac_rms_for_dc(chain: &Chain, registry: &[IoBinding], dc: f32, buffer: usize) -> f32 {
    let runtime = build_runtime(chain, registry);
    let sig = vec![dc; (SR as usize) * 2];
    let mut out_collected: Vec<f32> = Vec::with_capacity(sig.len());
    for chunk in sig.chunks(buffer) {
        process_input_f32(&runtime, 0, chunk, 1);
        let mut out = vec![0.0_f32; chunk.len() * 2];
        process_output_f32(&runtime, 0, &mut out, 2);
        for f in out.chunks_exact(2) {
            out_collected.push((f[0] + f[1]) * 0.5);
        }
    }
    let skip = SR as usize;
    let tail = &out_collected[skip..];
    let mean = tail.iter().sum::<f32>() / tail.len() as f32;
    (tail.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / tail.len() as f32).sqrt()
}

fn silent_residue(chain: &Chain, registry: &[IoBinding], buffer: usize) -> f32 {
    let runtime = build_runtime(chain, registry);
    let sig = vec![0.0_f32; (SR as usize) * 2];
    let mut out_collected: Vec<f32> = Vec::with_capacity(sig.len());
    for chunk in sig.chunks(buffer) {
        process_input_f32(&runtime, 0, chunk, 1);
        let mut out = vec![0.0_f32; chunk.len() * 2];
        process_output_f32(&runtime, 0, &mut out, 2);
        for f in out.chunks_exact(2) {
            out_collected.push((f[0] + f[1]) * 0.5);
        }
    }
    let skip = SR as usize;
    let tail = &out_collected[skip..];
    tail.iter().fold(0.0_f32, |a, &b| a.max(b.abs()))
}

fn lufs_delta_through_chain(chain: &Chain, registry: &[IoBinding], buffer: usize) -> f64 {
    use ebur128::{EbuR128, Mode};
    let target = DEFAULT_ELASTIC_TARGET.max(buffer);
    let runtime = Arc::new(build_chain_runtime_state(chain, SR, &[target], registry).expect("runtime"));
    let pink = pink_noise(SR as usize * 3, 0xDEAD_BEEF);
    let mut out_collected: Vec<f32> = Vec::with_capacity(pink.len());
    for chunk in pink.chunks(buffer) {
        process_input_f32(&runtime, 0, chunk, 1);
        let mut out = vec![0.0_f32; chunk.len() * 2];
        process_output_f32(&runtime, 0, &mut out, 2);
        for f in out.chunks_exact(2) {
            out_collected.push((f[0] + f[1]) * 0.5);
        }
    }
    let skip = SR as usize;
    let mut m_in = EbuR128::new(1, SR as u32, Mode::I).unwrap();
    m_in.add_frames_f32(&pink[skip..]).unwrap();
    let mut m_out = EbuR128::new(1, SR as u32, Mode::I).unwrap();
    m_out.add_frames_f32(&out_collected[skip..]).unwrap();
    m_out.loudness_global().unwrap() - m_in.loudness_global().unwrap()
}

fn bare_chain_for(id: &str) -> (Chain, Vec<IoBinding>) {
    chain_with_blocks(
        id,
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    )
}

fn sine_2s(freq: f32, amp: f32) -> Vec<f32> {
    (0..(SR as usize) * 2)
        .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / SR).sin())
        .collect()
}

// ── M.1 THD+N across buffer sizes (10 tests, 1 kHz @ 0.5) ───────
macro_rules! buf_thd_test {
    ($name:ident, $buf:expr) => {
        #[test]
        fn $name() {
            let (chain, registry) = bare_chain_for(stringify!($name));
            let sig = sine_2s(1_000.0, 0.5);
            let thd = thd_n_db_through_chain(&chain, &registry, &sig, $buf);
            eprintln!("[buffer={}] THD+N = {thd:.2} dB", $buf);
            assert!(thd < -60.0, "buffer={} THD+N {thd:.2} dB ≥ -60", $buf);
        }
    };
}
buf_thd_test!(m01_buf_64, 64);
buf_thd_test!(m02_buf_128, 128);
buf_thd_test!(m03_buf_192, 192);
buf_thd_test!(m04_buf_256, 256);
buf_thd_test!(m05_buf_384, 384);
buf_thd_test!(m06_buf_512, 512);
buf_thd_test!(m07_buf_768, 768);
buf_thd_test!(m08_buf_1024, 1024);
buf_thd_test!(m09_buf_1536, 1536);
buf_thd_test!(m10_buf_2048, 2048);

// ── M.2 THD+N across signal LEVELS at 512-frame buffer (5 tests) ──
macro_rules! lvl_thd_test {
    ($name:ident, $lvl:expr) => {
        #[test]
        fn $name() {
            let (chain, registry) = bare_chain_for(stringify!($name));
            let sig = sine_2s(1_000.0, $lvl);
            let thd = thd_n_db_through_chain(&chain, &registry, &sig, 512);
            eprintln!("[level={}] THD+N = {thd:.2} dB", $lvl);
            assert!(thd < -60.0, "level={} THD+N {thd:.2} dB ≥ -60", $lvl);
        }
    };
}
lvl_thd_test!(m11_level_0_1, 0.1);
lvl_thd_test!(m12_level_0_3, 0.3);
lvl_thd_test!(m13_level_0_5, 0.5);
lvl_thd_test!(m14_level_0_7, 0.7);
lvl_thd_test!(m15_level_0_9, 0.9);

// ── M.3 THD+N across FREQUENCIES at 512-frame buffer (5 tests) ────
macro_rules! freq_thd_test {
    ($name:ident, $f:expr) => {
        #[test]
        fn $name() {
            let (chain, registry) = bare_chain_for(stringify!($name));
            let sig = sine_2s($f, 0.5);
            let thd = thd_n_db_at_freq_through_chain(&chain, &registry, &sig, 512, $f);
            eprintln!("[freq={} Hz] THD+N = {thd:.2} dB", $f);
            assert!(thd < -60.0, "freq={} Hz THD+N {thd:.2} dB ≥ -60", $f);
        }
    };
}
freq_thd_test!(m16_freq_100, 100.0);
// Issue #496: use freqs with integer-cycle period at 48 kHz to avoid
// FFT leakage (220/440 Hz period is ~218/109 samples — non-integer).
freq_thd_test!(m17_freq_200, 200.0); // period = 240
freq_thd_test!(m18_freq_480, 480.0); // period = 100
freq_thd_test!(m19_freq_1000, 1_000.0);
freq_thd_test!(m20_freq_4000, 4_000.0);

// ── M.4 DC injection produces AC noise (5 tests) ────────────────
macro_rules! dc_ac_test {
    ($name:ident, $dc:expr) => {
        #[test]
        fn $name() {
            let (chain, registry) = bare_chain_for(stringify!($name));
            let ac = ac_rms_for_dc(&chain, &registry, $dc, 512);
            eprintln!("[DC={}] AC rms out = {ac:.6e}", $dc);
            assert!(
                ac < 5e-4,
                "DC={} produced AC rms {ac:.6e} (>-66 dBFS = audible hiss)",
                $dc
            );
        }
    };
}
dc_ac_test!(m21_dc_0_1, 0.1_f32);
dc_ac_test!(m22_dc_0_3, 0.3_f32);
dc_ac_test!(m23_dc_0_5, 0.5_f32);
dc_ac_test!(m24_dc_0_7, 0.7_f32);
dc_ac_test!(m25_dc_neg_0_3, -0.3_f32);

// ── M.5 Silent input → silent output across buffer sizes (5) ────
macro_rules! silent_test {
    ($name:ident, $buf:expr) => {
        #[test]
        fn $name() {
            let (chain, registry) = bare_chain_for(stringify!($name));
            let peak = silent_residue(&chain, &registry, $buf);
            eprintln!("[silent buf={}] peak = {peak:.6}", $buf);
            assert!(peak < 1e-6, "silent buf={} produced peak {peak:.6}", $buf);
        }
    };
}
silent_test!(m26_silent_buf_128, 128);
silent_test!(m27_silent_buf_256, 256);
silent_test!(m28_silent_buf_512, 512);
silent_test!(m29_silent_buf_1024, 1024);
silent_test!(m30_silent_buf_2048, 2048);

// ── M.6 LUFS preservation across buffer sizes (5) ───────────────
macro_rules! lufs_test {
    ($name:ident, $buf:expr) => {
        #[test]
        fn $name() {
            let (chain, registry) = bare_chain_for(stringify!($name));
            let d = lufs_delta_through_chain(&chain, &registry, $buf);
            eprintln!("[lufs buf={}] delta = {d:+.2} dB", $buf);
            assert!(d.abs() < 1.0, "lufs buf={} delta {d:+.2} dB", $buf);
        }
    };
}
lufs_test!(m31_lufs_buf_128, 128);
lufs_test!(m32_lufs_buf_256, 256);
lufs_test!(m33_lufs_buf_512, 512);
lufs_test!(m34_lufs_buf_1024, 1024);
lufs_test!(m35_lufs_buf_2048, 2048);

// ─────────────────────────────────────────────────────────────────────────
// N. Mono→Stereo broadcast — 30 tests probing L=R bit-equality across
//    levels, frequencies, signal shapes, and buffer sizes.
// ─────────────────────────────────────────────────────────────────────────

fn max_lr_drift(chain: &Chain, registry: &[IoBinding], sig: &[f32], buffer: usize) -> f32 {
    let runtime = build_runtime(chain, registry);
    let mut max_drift = 0.0_f32;
    let mut callback_idx = 0;
    for chunk in sig.chunks(buffer) {
        process_input_f32(&runtime, 0, chunk, 1);
        let mut out = vec![0.0_f32; chunk.len() * 2];
        process_output_f32(&runtime, 0, &mut out, 2);
        // Skip first 4 callbacks to avoid fade-in artifacts.
        if callback_idx >= 4 {
            for f in out.chunks_exact(2) {
                let d = (f[0] - f[1]).abs();
                if d > max_drift {
                    max_drift = d;
                }
            }
        }
        callback_idx += 1;
    }
    max_drift
}

macro_rules! bcast_sine_test {
    ($name:ident, $f:expr, $amp:expr, $buf:expr) => {
        #[test]
        fn $name() {
            let (chain, registry) = bare_chain_for(stringify!($name));
            let sig: Vec<f32> = (0..(SR as usize))
                .map(|i| $amp * (2.0 * std::f32::consts::PI * $f * i as f32 / SR).sin())
                .collect();
            let d = max_lr_drift(&chain, &registry, &sig, $buf);
            eprintln!(
                "[bcast f={} amp={} buf={}] max drift = {d:.6}",
                $f, $amp, $buf
            );
            assert!(d < 1e-5, "L vs R drift {d:.6} ≥ 1e-5");
        }
    };
}

bcast_sine_test!(n01_b_100hz_0_3_buf_128, 100.0, 0.3, 128);
bcast_sine_test!(n02_b_220hz_0_3_buf_128, 220.0, 0.3, 128);
bcast_sine_test!(n03_b_440hz_0_3_buf_128, 440.0, 0.3, 128);
bcast_sine_test!(n04_b_1khz_0_3_buf_128, 1_000.0, 0.3, 128);
bcast_sine_test!(n05_b_4khz_0_3_buf_128, 4_000.0, 0.3, 128);
bcast_sine_test!(n06_b_220hz_0_1_buf_512, 220.0, 0.1, 512);
bcast_sine_test!(n07_b_220hz_0_5_buf_512, 220.0, 0.5, 512);
bcast_sine_test!(n08_b_220hz_0_8_buf_512, 220.0, 0.8, 512);
bcast_sine_test!(n09_b_220hz_0_95_buf_512, 220.0, 0.95, 512);
bcast_sine_test!(n10_b_1khz_0_3_buf_64, 1_000.0, 0.3, 64);
bcast_sine_test!(n11_b_1khz_0_3_buf_256, 1_000.0, 0.3, 256);
bcast_sine_test!(n12_b_1khz_0_3_buf_768, 1_000.0, 0.3, 768);
bcast_sine_test!(n13_b_1khz_0_3_buf_2048, 1_000.0, 0.3, 2048);

macro_rules! bcast_signal_test {
    ($name:ident, $sig_expr:expr, $buf:expr) => {
        #[test]
        fn $name() {
            let (chain, registry) = bare_chain_for(stringify!($name));
            let sig: Vec<f32> = $sig_expr;
            let d = max_lr_drift(&chain, &registry, &sig, $buf);
            eprintln!("[{} buf={}] max drift = {d:.6}", stringify!($name), $buf);
            assert!(d < 1e-5, "L vs R drift {d:.6} ≥ 1e-5");
        }
    };
}

bcast_signal_test!(n14_b_dc_pos, vec![0.3_f32; (SR as usize) * 2], 256);
bcast_signal_test!(n15_b_dc_neg, vec![-0.4_f32; (SR as usize) * 2], 256);
bcast_signal_test!(n16_b_silence, vec![0.0_f32; (SR as usize) * 2], 256);
bcast_signal_test!(n17_b_pink_noise, pink_noise((SR as usize) * 2, 0xCAFE), 256);
bcast_signal_test!(
    n18_b_two_tone,
    (0..(SR as usize))
        .map(
            |i| 0.3 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / SR).sin()
                + 0.3 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SR).sin()
        )
        .collect(),
    256
);
bcast_signal_test!(
    n19_b_ramp_up,
    (0..(SR as usize))
        .map(|i| 0.8 * (i as f32 / SR as f32))
        .collect(),
    256
);
bcast_signal_test!(
    n20_b_pluck,
    (0..(SR as usize))
        .map(|i| {
            let t = i as f32 / SR;
            0.5 * (-t / 0.4).exp() * (2.0 * std::f32::consts::PI * 150.0 * t).sin()
        })
        .collect(),
    256
);
bcast_signal_test!(
    n21_b_impulse,
    {
        let mut v = vec![0.0_f32; SR as usize];
        v[128] = 0.7;
        v
    },
    256
);
bcast_signal_test!(
    n22_b_square,
    (0..(SR as usize))
        .map(|i| if (i / 240) % 2 == 0 { 0.3 } else { -0.3 })
        .collect(),
    256
);
bcast_signal_test!(
    n23_b_sawtooth,
    (0..(SR as usize))
        .map(|i| 0.4 * (((i as f32 % 240.0) / 240.0) * 2.0 - 1.0))
        .collect(),
    256
);
bcast_signal_test!(
    n24_b_triangle,
    (0..(SR as usize))
        .map(|i| {
            let p = (i as f32 % 240.0) / 240.0;
            0.4 * (1.0 - (2.0 * p - 1.0).abs() * 2.0)
        })
        .collect(),
    256
);

bcast_sine_test!(n25_b_100hz_0_5_buf_64, 100.0, 0.5, 64);
bcast_sine_test!(n26_b_100hz_0_5_buf_2048, 100.0, 0.5, 2048);
bcast_sine_test!(n27_b_4khz_0_5_buf_64, 4_000.0, 0.5, 64);
bcast_sine_test!(n28_b_4khz_0_5_buf_2048, 4_000.0, 0.5, 2048);
bcast_sine_test!(n29_b_8khz_0_3_buf_512, 8_000.0, 0.3, 512);
bcast_sine_test!(n30_b_60hz_0_5_buf_512, 60.0, 0.5, 512);

// ─────────────────────────────────────────────────────────────────────────
// O. Fade-in ramp — 30 tests checking the ramp does not leak past its
//    documented duration, does not corrupt audio after release, and
//    does not introduce harmonics.
// ─────────────────────────────────────────────────────────────────────────

/// THD+N computed using a specified skip duration (samples). If THD+N
/// keeps improving as skip grows, the fade-in is leaking — its end
/// boundary should be a hard release into transparency.
fn thd_with_skip(chain: &Chain, registry: &[IoBinding], freq: f32, amp: f32, buffer: usize, skip: usize) -> f32 {
    use rustfft::{num_complex::Complex, FftPlanner};
    let target = DEFAULT_ELASTIC_TARGET.max(buffer);
    let runtime = Arc::new(build_chain_runtime_state(chain, SR, &[target], registry).expect("runtime"));
    let n: usize = (SR as usize) * 3;
    let sig: Vec<f32> = (0..n)
        .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / SR).sin())
        .collect();
    let mut out_collected: Vec<f32> = Vec::with_capacity(sig.len());
    for chunk in sig.chunks(buffer) {
        process_input_f32(&runtime, 0, chunk, 1);
        let mut out = vec![0.0_f32; chunk.len() * 2];
        process_output_f32(&runtime, 0, &mut out, 2);
        for f in out.chunks_exact(2) {
            out_collected.push((f[0] + f[1]) * 0.5);
        }
    }
    // Issue #496 measurement fix: truncate to integer cycles, no zero-pad.
    let cycle_samples = (SR / freq).round().max(1.0) as usize;
    let usable_total = out_collected.len() - skip;
    let usable = (usable_total / cycle_samples) * cycle_samples;
    let tail = &out_collected[skip..skip + usable];
    let nfft = tail.len();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(nfft);
    let mut buf: Vec<Complex<f32>> = tail.iter().map(|&s| Complex::new(s, 0.0)).collect();
    fft.process(&mut buf);
    let bin_hz = SR / nfft as f32;
    let fb = (freq / bin_hz).round() as usize;
    let fundamental: f32 = (fb.saturating_sub(1)..=fb + 1)
        .map(|b| buf[b].norm_sqr())
        .sum();
    let total: f32 = buf[..nfft / 2].iter().map(|c| c.norm_sqr()).sum();
    10.0 * ((total - fundamental).max(1e-12) / fundamental).log10()
}

macro_rules! fade_skip_test {
    ($name:ident, $skip_ms:expr, $buf:expr) => {
        #[test]
        fn $name() {
            let (chain, registry) = bare_chain_for(stringify!($name));
            let skip = (SR as usize) * $skip_ms / 1000;
            let thd = thd_with_skip(&chain, &registry, 1_000.0, 0.5, $buf, skip);
            eprintln!("[skip {} ms, buf {}] THD+N = {thd:.2} dB", $skip_ms, $buf);
            assert!(
                thd < -60.0,
                "skip={}ms buf={} THD+N {thd:.2} dB ≥ -60",
                $skip_ms,
                $buf
            );
        }
    };
}
fade_skip_test!(o01_skip_50ms_buf_128, 50, 128);
fade_skip_test!(o02_skip_100ms_buf_128, 100, 128);
fade_skip_test!(o03_skip_200ms_buf_128, 200, 128);
fade_skip_test!(o04_skip_500ms_buf_128, 500, 128);
fade_skip_test!(o05_skip_1s_buf_128, 1_000, 128);
fade_skip_test!(o06_skip_50ms_buf_512, 50, 512);
fade_skip_test!(o07_skip_100ms_buf_512, 100, 512);
fade_skip_test!(o08_skip_200ms_buf_512, 200, 512);
fade_skip_test!(o09_skip_500ms_buf_512, 500, 512);
fade_skip_test!(o10_skip_1s_buf_512, 1_000, 512);
fade_skip_test!(o11_skip_50ms_buf_2048, 50, 2048);
fade_skip_test!(o12_skip_200ms_buf_2048, 200, 2048);
fade_skip_test!(o13_skip_500ms_buf_2048, 500, 2048);
fade_skip_test!(o14_skip_1s_buf_2048, 1_000, 2048);

// Across signal levels
fade_skip_test!(o15_skip_500ms_lvl_via_freq_100, 500, 256);
fade_skip_test!(o16_skip_500ms_lvl_via_freq_220, 500, 256);
fade_skip_test!(o17_skip_500ms_lvl_via_freq_440, 500, 256);
fade_skip_test!(o18_skip_500ms_lvl_via_freq_2k, 500, 256);
fade_skip_test!(o19_skip_500ms_lvl_via_freq_8k, 500, 256);

// Across many buffers at fixed 500 ms skip
fade_skip_test!(o20_skip_500ms_buf_64, 500, 64);
fade_skip_test!(o21_skip_500ms_buf_192, 500, 192);
fade_skip_test!(o22_skip_500ms_buf_384, 500, 384);
fade_skip_test!(o23_skip_500ms_buf_768, 500, 768);
fade_skip_test!(o24_skip_500ms_buf_1024, 1_024, 1_024);
fade_skip_test!(o25_skip_500ms_buf_1536, 500, 1_536);
fade_skip_test!(o26_skip_500ms_buf_4096, 500, 4_096);

// Skip much longer than any plausible fade
fade_skip_test!(o27_skip_2s_buf_512, 2_000, 512);
fade_skip_test!(o28_skip_2s_buf_2048, 2_000, 2_048);
fade_skip_test!(o29_skip_2s_buf_128, 2_000, 128);
fade_skip_test!(o30_skip_2s_buf_64, 2_000, 64);

// ─────────────────────────────────────────────────────────────────────────
// P. Sample format conversion math — 30 tests of the exact
//    i16/u16/i32 ↔ f32 expressions used by `stream_builder.rs`.
//    These don't go through the engine; they verify the math the cpal
//    callback runs is bijective and not the source of the swarm-of-bees
//    via bit-cast / off-by-one / wrap-around.
// ─────────────────────────────────────────────────────────────────────────

fn i16_to_f32(s: i16) -> f32 {
    s as f32 / i16::MAX as f32
}
fn f32_to_i16(s: f32) -> i16 {
    (s * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16
}
fn u16_to_f32(s: u16) -> f32 {
    (s as f32 / u16::MAX as f32) * 2.0 - 1.0
}
fn f32_to_u16(s: f32) -> u16 {
    ((s + 1.0) * 0.5 * u16::MAX as f32).clamp(0.0, u16::MAX as f32) as u16
}
fn i32_to_f32(s: i32) -> f32 {
    s as f32 / i32::MAX as f32
}
fn f32_to_i32(s: f32) -> i32 {
    (s * i32::MAX as f32).clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

#[test]
fn p01_i16_max_round_trip() {
    assert!((f32_to_i16(i16_to_f32(i16::MAX)) - i16::MAX).abs() <= 1);
}
#[test]
fn p02_i16_min_round_trip() {
    assert!((f32_to_i16(i16_to_f32(i16::MIN)) - i16::MIN).abs() <= 1);
}
#[test]
fn p03_i16_zero_round_trip() {
    assert_eq!(f32_to_i16(i16_to_f32(0)), 0);
}
#[test]
fn p04_i16_one_round_trip() {
    assert_eq!(f32_to_i16(i16_to_f32(1)), 1);
}
#[test]
fn p05_i16_neg_one_round_trip() {
    assert_eq!(f32_to_i16(i16_to_f32(-1)), -1);
}
#[test]
fn p06_i16_half_round_trip() {
    let v = i16::MAX / 2;
    assert!((f32_to_i16(i16_to_f32(v)) - v).abs() <= 1);
}
#[test]
fn p07_i16_neg_half_round_trip() {
    let v = i16::MIN / 2;
    assert!((f32_to_i16(i16_to_f32(v)) - v).abs() <= 1);
}
#[test]
fn p08_i16_clamps_above_unity() {
    assert_eq!(f32_to_i16(2.0), i16::MAX);
}
#[test]
fn p09_i16_clamps_below_minus_unity() {
    assert_eq!(f32_to_i16(-2.0), i16::MIN);
}
#[test]
fn p10_i16_to_f32_bound() {
    for v in [-32768i16, -1, 0, 1, 32767] {
        let x = i16_to_f32(v);
        assert!(x >= -1.001 && x <= 1.001, "v={v} x={x}");
    }
}

#[test]
fn p11_u16_zero_maps_to_minus_one() {
    assert!((u16_to_f32(0) + 1.0).abs() < 1e-4);
}
#[test]
fn p12_u16_max_maps_to_plus_one() {
    assert!((u16_to_f32(u16::MAX) - 1.0).abs() < 1e-4);
}
#[test]
fn p13_u16_mid_maps_near_zero() {
    let v = u16::MAX / 2;
    assert!(u16_to_f32(v).abs() < 1e-4);
}
#[test]
fn p14_u16_round_trip_zero() {
    assert_eq!(f32_to_u16(-1.0), 0);
}
#[test]
fn p15_u16_round_trip_max() {
    assert_eq!(f32_to_u16(1.0), u16::MAX);
}
#[test]
fn p16_u16_round_trip_mid() {
    let v = u16::MAX / 2;
    let back = f32_to_u16(u16_to_f32(v));
    assert!((back as i32 - v as i32).abs() <= 1);
}
#[test]
fn p17_u16_clamps_above_unity() {
    assert_eq!(f32_to_u16(2.0), u16::MAX);
}
#[test]
fn p18_u16_clamps_below_minus_unity() {
    assert_eq!(f32_to_u16(-2.0), 0);
}
#[test]
fn p19_u16_to_f32_bound() {
    for v in [0u16, 1, u16::MAX / 2, u16::MAX] {
        let x = u16_to_f32(v);
        assert!(x >= -1.001 && x <= 1.001);
    }
}
#[test]
fn p20_u16_round_trip_dense() {
    for v in (0..u16::MAX).step_by(257) {
        let back = f32_to_u16(u16_to_f32(v));
        assert!((back as i32 - v as i32).abs() <= 1, "v={v} back={back}");
    }
}

#[test]
fn p21_i32_zero_round_trip() {
    assert_eq!(f32_to_i32(i32_to_f32(0)), 0);
}
#[test]
fn p22_i32_max_round_trip_bounded() {
    let x = i32_to_f32(i32::MAX);
    assert!((x - 1.0).abs() < 1e-6);
}
#[test]
fn p23_i32_min_round_trip_bounded() {
    let x = i32_to_f32(i32::MIN);
    assert!((x + 1.0).abs() < 1e-3);
}
#[test]
fn p24_i32_clamps_above_unity() {
    assert_eq!(f32_to_i32(2.0), i32::MAX);
}
#[test]
fn p25_i32_clamps_below_minus_unity() {
    assert_eq!(f32_to_i32(-2.0), i32::MIN);
}
#[test]
fn p26_i32_to_f32_bound() {
    for v in [i32::MIN, -1, 0, 1, i32::MAX] {
        let x = i32_to_f32(v);
        assert!(x >= -1.001 && x <= 1.001, "v={v} x={x}");
    }
}
#[test]
fn p27_i32_unity_round_trip() {
    assert_eq!(f32_to_i32(1.0), i32::MAX);
}
#[test]
fn p28_i32_neg_unity_round_trip() {
    assert_eq!(f32_to_i32(-1.0), i32::MIN);
}
#[test]
fn p29_i32_below_unity_is_within() {
    for &v in &[0.1_f32, 0.5, 0.9, -0.3] {
        assert!(f32_to_i32(v).abs() < i32::MAX);
    }
}
#[test]
fn p30_i32_subnormal_safe() {
    let x = i32_to_f32(1);
    let back = f32_to_i32(x);
    assert!((back - 1).abs() <= 1);
}

#[test]
fn diag_thd_with_single_callback_push_pop() {
    use rustfft::{num_complex::Complex, FftPlanner};
    let (chain, registry) = bare_chain_for("diag_single");
    let n: usize = 16_384;
    let sig: Vec<f32> = (0..n)
        .map(|i| 0.5_f32 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin())
        .collect();
    let runtime = build_runtime(&chain, &registry);
    // Single big push + single big pop.
    process_input_f32(&runtime, 0, &sig, 1);
    let mut out_st = vec![0.0_f32; n * 2];
    process_output_f32(&runtime, 0, &mut out_st, 2);
    let out: Vec<f32> = out_st
        .chunks_exact(2)
        .map(|f| (f[0] + f[1]) * 0.5_f32)
        .collect();
    // Skip fade-in.
    let skip = (crate::runtime_state::FADE_IN_FRAMES + 16).min(out.len() / 4);
    let tail = &out[skip..];
    let nfft = tail.len().next_power_of_two();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(nfft);
    let mut buf: Vec<Complex<f32>> = tail
        .iter()
        .map(|&s| Complex::new(s, 0.0))
        .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
        .take(nfft)
        .collect();
    fft.process(&mut buf);
    let bin_hz = SR / nfft as f32;
    let fb = (1_000.0 / bin_hz).round() as usize;
    let fundamental: f32 = (fb.saturating_sub(3)..=fb + 3)
        .map(|b| buf[b].norm_sqr())
        .sum();
    let total: f32 = buf[..nfft / 2].iter().map(|c| c.norm_sqr()).sum();
    let thd_n_db = 10.0 * ((total - fundamental).max(1e-12) / fundamental).log10();
    eprintln!("\n=== diag SINGLE callback push/pop ===\n  THD+N = {thd_n_db:.2} dB  (signal length = {n} samples)");
}

#[test]
fn diag_multi_callback_bit_exact_chunks_of_64() {
    let (chain, registry) = bare_chain_for("diag_multi");
    let n: usize = 4096;
    let sig: Vec<f32> = (0..n)
        .map(|i| 0.5_f32 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin())
        .collect();
    let runtime = build_runtime(&chain, &registry);
    let buffer = 64;
    let mut out_collected: Vec<f32> = Vec::with_capacity(n);
    for chunk in sig.chunks(buffer) {
        process_input_f32(&runtime, 0, chunk, 1);
        let mut out = vec![0.0_f32; chunk.len() * 2];
        process_output_f32(&runtime, 0, &mut out, 2);
        for f in out.chunks_exact(2) {
            out_collected.push((f[0] + f[1]) * 0.5_f32);
        }
    }
    // Count exact mismatches per region.
    let skip = 128_usize; // past FADE_IN_FRAMES
    let mut mismatches = 0_usize;
    let mut worst: (usize, f32, f32) = (0, 0.0, 0.0);
    for i in skip..n {
        let want = sig[i];
        let got = out_collected[i];
        let d = (got - want).abs();
        if d > 1e-5 {
            mismatches += 1;
            if d > worst.1.abs().max(worst.2.abs()).max(0.0) {
                worst = (i, want, got);
            }
        }
    }
    eprintln!("\n=== diag MULTI 64-frame callbacks (skip {skip}) ===");
    eprintln!("  total frames after skip = {}", n - skip);
    eprintln!("  mismatches (|delta| > 1e-5) = {mismatches}");
    eprintln!(
        "  worst @ i={}: want={:+.6} got={:+.6}",
        worst.0, worst.1, worst.2
    );
    // Print 16 around worst.
    let around = worst.0.saturating_sub(8);
    eprintln!("  around worst (i={around}..{}):", around + 16);
    for i in around..(around + 16).min(n) {
        eprintln!(
            "   {i:>5}: want={:>+9.6}  got={:>+9.6}  delta={:>+9.6}",
            sig[i],
            out_collected[i],
            out_collected[i] - sig[i]
        );
    }
}

#[test]
fn diag_print_first_chunk_of_bare_chain_output() {
    let (chain, registry) = bare_chain_for("diag");
    let n: usize = 256;
    let sig: Vec<f32> = (0..n)
        .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin())
        .collect();
    let runtime = build_runtime(&chain, &registry);
    // Push and pop a few callbacks to get past fade-in.
    for _ in 0..4 {
        process_input_f32(&runtime, 0, &sig, 1);
        let mut out = vec![0.0_f32; n * 2];
        process_output_f32(&runtime, 0, &mut out, 2);
    }
    // Capture next callback.
    process_input_f32(&runtime, 0, &sig, 1);
    let mut out = vec![0.0_f32; n * 2];
    process_output_f32(&runtime, 0, &mut out, 2);
    eprintln!("\n=== diag: first 16 stereo frames after warmup ===");
    eprintln!("  i:  in            outL          outR          delta");
    for i in 0..16 {
        let want = sig[i];
        let l = out[i * 2];
        let r = out[i * 2 + 1];
        eprintln!(
            " {i:>3}: {want:>+10.6}  {l:>+10.6}  {r:>+10.6}  L-want={:+.6}",
            l - want
        );
    }
}
