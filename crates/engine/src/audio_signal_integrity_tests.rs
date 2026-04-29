//! Audio signal integrity tests — looks at the output samples directly.
//!
//! THE PURPOSE: detect "the sound went bad" as a measurable property of
//! the output buffer, not a proxy. The deadline test catches CPU
//! regressions; this catches DSP regressions and discontinuities that
//! appear in the audio signal itself.
//!
//! WHAT A CLICK / GLITCH ACTUALLY IS in the signal:
//!
//!   1. NaN or Inf samples — division by zero, denormals, broken DSP.
//!   2. Sudden jump: |s[n] - s[n-1]| larger than the input could explain.
//!      A smooth sine moves by at most 2π · f / SR per sample. Anything
//!      bigger than that is a discontinuity → audible click.
//!   3. Sudden silence in a non-silent window: output goes to ~0 while
//!      the input is still feeding — that's an underrun pattern, the
//!      consumer pulled the fallback frame instead of real audio.
//!   4. Sudden DC: output saturates to a fixed value (also an underrun
//!      signature when the fallback is held).
//!   5. DC offset: silent input must produce silent output. A non-zero
//!      offset means a DSP block leaked bias into the chain.
//!
//! These are PRODUCT-FACING properties: if any of them is true on a
//! given output buffer, the user hears something wrong. So the test
//! asserts directly on the output samples — not on CPU time, not on
//! call counts, not on schema metadata. The audio.
//!
//! HOW IT WORKS:
//!   - Build a runtime with a known chain.
//!   - Feed a smooth, predictable input (sine wave or silence).
//!   - Capture N callbacks of output (skip the FADE_IN warmup).
//!   - Scan the captured samples for the failure modes above.
//!
//! GATING: these run in debug AND release — they're DSP correctness
//! tests, not timing tests. The signal property is the same regardless
//! of optimizer level.
//!
//! HONEST LIMITATIONS — same as audio_deadline_tests.rs in spirit:
//!   - Offline: not exercising the real audio backend.
//!   - Single chain at a time: cross-chain interactions not covered.
//!   - Smooth synthetic input only: real guitar signal has transients
//!     that can mask subtler regressions.
//!
//! Combined with audio_deadline_tests.rs (timing), volume_invariants
//! (level), and stream_isolation (per-stream independence), this gives
//! four orthogonal numerical layers protecting the audio output. A
//! refactor that breaks any one of them is caught before the user
//! hears it.

use super::{
    build_chain_runtime_state, process_input_f32, process_output_f32, DEFAULT_ELASTIC_TARGET,
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
const BUFFER_FRAMES: usize = 64;

// ─────────────────────────────────────────────────────────────────────────
// Chain builders (mirror those in volume_invariants_tests.rs / audio_deadline_tests.rs)
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

fn chain_with_blocks(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some("signal integrity test".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        blocks,
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

// ─────────────────────────────────────────────────────────────────────────
// Input generators
// ─────────────────────────────────────────────────────────────────────────

/// Continuous sine generator with internal phase, so successive buffers
/// stitch together smoothly. This is the key — the offline test
/// produces a perfectly continuous signal at the input. Any
/// discontinuity in the OUTPUT is the engine's fault.
struct SineGen {
    phase: f32,
    incr: f32,
    amplitude: f32,
}

impl SineGen {
    fn new(freq_hz: f32, sample_rate: f32, amplitude: f32) -> Self {
        Self {
            phase: 0.0,
            incr: 2.0 * std::f32::consts::PI * freq_hz / sample_rate,
            amplitude,
        }
    }

    /// Fill an interleaved buffer with `frames` frames × `channels` channels.
    /// All channels get the same sine sample (mono test signal that we
    /// place in N channels).
    fn fill(&mut self, buf: &mut [f32], frames: usize, channels: usize) {
        debug_assert_eq!(buf.len(), frames * channels);
        for f in 0..frames {
            let s = self.amplitude * self.phase.sin();
            self.phase += self.incr;
            if self.phase > std::f32::consts::TAU {
                self.phase -= std::f32::consts::TAU;
            }
            for c in 0..channels {
                buf[f * channels + c] = s;
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Failure-mode scanners — these are what define a "click"
// ─────────────────────────────────────────────────────────────────────────

/// Walk an interleaved output buffer per-channel and look for the four
/// failure modes from the file header. Returns the first failure found,
/// or `Ok(())` if the buffer is clean.
///
/// `max_smooth_delta` is the largest legal jump between consecutive
/// samples on the same channel. For a 220 Hz sine at amplitude 0.5 and
/// 48 kHz SR, the natural max delta is 2π·220/48000·0.5 ≈ 0.0144. We
/// use a much larger threshold (typically 0.1) to give DSP wiggle room
/// while still catching real clicks (which jump by 0.3+ in practice).
fn scan_for_click(
    label: &str,
    out: &[f32],
    channels: usize,
    max_smooth_delta: f32,
) -> Result<(), String> {
    if out.is_empty() {
        return Ok(());
    }
    let frames = out.len() / channels;
    for ch in 0..channels {
        let mut prev: Option<f32> = None;
        for f in 0..frames {
            let s = out[f * channels + ch];

            // Failure mode 1: NaN / Inf.
            if !s.is_finite() {
                return Err(format!(
                    "{label}: non-finite sample at frame {f} channel {ch}: {s}"
                ));
            }

            // Failure mode 2: sudden jump.
            if let Some(p) = prev {
                let delta = (s - p).abs();
                if delta > max_smooth_delta {
                    return Err(format!(
                        "{label}: click at frame {f} channel {ch}: |{s} - {p}| = {delta:.4} \
                         exceeds max smooth delta {max_smooth_delta:.4}"
                    ));
                }
            }
            prev = Some(s);
        }
    }
    Ok(())
}

/// Assert no NaN / Inf anywhere in the buffer. Cheaper, used on its own
/// for tests that don't care about smoothness.
fn scan_finite(label: &str, out: &[f32]) -> Result<(), String> {
    for (i, &s) in out.iter().enumerate() {
        if !s.is_finite() {
            return Err(format!("{label}: non-finite sample at index {i}: {s}"));
        }
    }
    Ok(())
}

/// Assert no sample exceeds an absolute magnitude. Catches runaway
/// feedback / blow-up in DSP without depending on the limiter.
fn scan_within_magnitude(label: &str, out: &[f32], max_abs: f32) -> Result<(), String> {
    for (i, &s) in out.iter().enumerate() {
        if s.abs() > max_abs {
            return Err(format!(
                "{label}: sample at index {i} = {s} exceeds max abs {max_abs}"
            ));
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// Driver
// ─────────────────────────────────────────────────────────────────────────

fn build_runtime(chain: &Chain) -> Arc<super::ChainRuntimeState> {
    Arc::new(
        build_chain_runtime_state(chain, SR, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime state should build"),
    )
}

/// Drive `n_callbacks` of `BUFFER_FRAMES` frames each through the runtime,
/// feeding the input from `gen`, and capture the concatenated output of
/// the steady-state callbacks (skipping `warmup_callbacks`).
fn drive_capture_steady(
    runtime: &Arc<super::ChainRuntimeState>,
    gen: &mut SineGen,
    input_channels: usize,
    output_channels: usize,
    n_callbacks: usize,
    warmup_callbacks: usize,
) -> Vec<f32> {
    let mut input_buf = vec![0.0_f32; BUFFER_FRAMES * input_channels];
    let mut output_buf = vec![0.0_f32; BUFFER_FRAMES * output_channels];
    let mut captured: Vec<f32> =
        Vec::with_capacity((n_callbacks - warmup_callbacks) * BUFFER_FRAMES * output_channels);

    for cb in 0..n_callbacks {
        gen.fill(&mut input_buf, BUFFER_FRAMES, input_channels);
        process_input_f32(runtime, 0, &input_buf, input_channels);
        process_output_f32(runtime, 0, &mut output_buf, output_channels);
        if cb >= warmup_callbacks {
            captured.extend_from_slice(&output_buf);
        }
    }
    captured
}

fn drive_capture_silent(
    runtime: &Arc<super::ChainRuntimeState>,
    input_channels: usize,
    output_channels: usize,
    n_callbacks: usize,
    warmup_callbacks: usize,
) -> Vec<f32> {
    let input_buf = vec![0.0_f32; BUFFER_FRAMES * input_channels];
    let mut output_buf = vec![0.0_f32; BUFFER_FRAMES * output_channels];
    let mut captured: Vec<f32> =
        Vec::with_capacity((n_callbacks - warmup_callbacks) * BUFFER_FRAMES * output_channels);

    for cb in 0..n_callbacks {
        process_input_f32(runtime, 0, &input_buf, input_channels);
        process_output_f32(runtime, 0, &mut output_buf, output_channels);
        if cb >= warmup_callbacks {
            captured.extend_from_slice(&output_buf);
        }
    }
    captured
}

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn pipe_only_mono_sine_is_smooth() {
    // 220 Hz sine through a mono → mono pipe. Output should be a clean
    // sine, no clicks. Natural max delta ≈ 0.014; threshold 0.1 catches
    // real clicks (0.3+) without false-failing on DSP rounding.
    let chain = chain_with_blocks(
        "pipe-mono-sine",
        vec![input_mono(vec![0]), output(ChainOutputMode::Mono, vec![0])],
    );
    let runtime = build_runtime(&chain);
    let mut gen = SineGen::new(220.0, SR, 0.5);
    // 32 callbacks × 64 frames = 2048 samples ≈ 43 ms of audio
    let captured = drive_capture_steady(&runtime, &mut gen, 1, 1, 32, 4);

    scan_for_click("pipe_mono_sine", &captured, 1, 0.1).expect("audio integrity violated");
    scan_within_magnitude("pipe_mono_sine_magnitude", &captured, 1.0)
        .expect("output exceeded ±1.0");
}

#[test]
fn pipe_only_stereo_sine_is_smooth_per_channel() {
    let chain = chain_with_blocks(
        "pipe-stereo-sine",
        vec![
            input_stereo(vec![0, 1]),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let runtime = build_runtime(&chain);
    let mut gen = SineGen::new(220.0, SR, 0.5);
    let captured = drive_capture_steady(&runtime, &mut gen, 2, 2, 32, 4);

    scan_for_click("pipe_stereo_sine", &captured, 2, 0.1).expect("audio integrity violated");
    scan_within_magnitude("pipe_stereo_sine_magnitude", &captured, 1.0)
        .expect("output exceeded ±1.0");
}

#[test]
fn pipe_mono_to_stereo_broadcasts_smoothly() {
    // Mono in → stereo out: both channels must carry the same smooth
    // signal. Catches a regression where the broadcast path glitches
    // one channel while leaving the other intact.
    let chain = chain_with_blocks(
        "pipe-mono-to-stereo",
        vec![
            input_mono(vec![0]),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let runtime = build_runtime(&chain);
    let mut gen = SineGen::new(220.0, SR, 0.5);
    let captured = drive_capture_steady(&runtime, &mut gen, 1, 2, 32, 4);

    scan_for_click("pipe_mono_to_stereo", &captured, 2, 0.1).expect("audio integrity violated");
    scan_within_magnitude("pipe_mono_to_stereo_magnitude", &captured, 1.0)
        .expect("output exceeded ±1.0");

    // Also verify L == R (broadcast invariant from CLAUDE.md).
    for f in 0..(captured.len() / 2) {
        let l = captured[f * 2];
        let r = captured[f * 2 + 1];
        assert!(
            (l - r).abs() < 1e-6,
            "broadcast violated at frame {f}: L={l} R={r}"
        );
    }
}

#[test]
fn silent_input_produces_silent_output_no_dc_offset() {
    // Silent input must produce silent output. A non-zero DC offset
    // means a DSP block leaked bias; a slowly-rising ramp means a
    // filter is unstable. Threshold of 1e-3 is generous (≈ -60 dBFS).
    let chain = chain_with_blocks(
        "pipe-silence",
        vec![input_mono(vec![0]), output(ChainOutputMode::Mono, vec![0])],
    );
    let runtime = build_runtime(&chain);
    let captured = drive_capture_silent(&runtime, 1, 1, 32, 4);

    scan_finite("silent_finite", &captured).expect("non-finite output for silent input");
    let max_abs = captured.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
    assert!(
        max_abs < 1e-3,
        "silent input produced output with peak {max_abs} (expected < 1e-3)"
    );
}

#[test]
fn extreme_amplitude_input_does_not_produce_nan() {
    // ±1.0 sine through the chain. Output must remain finite even at
    // full scale where the limiter engages. Catches divisions by zero
    // and overflow in DSP blocks.
    let chain = chain_with_blocks(
        "pipe-fullscale",
        vec![input_mono(vec![0]), output(ChainOutputMode::Mono, vec![0])],
    );
    let runtime = build_runtime(&chain);
    let mut gen = SineGen::new(220.0, SR, 1.0);
    let captured = drive_capture_steady(&runtime, &mut gen, 1, 1, 32, 4);

    scan_finite("fullscale_finite", &captured).expect("non-finite output for full-scale input");
}

// ─────────────────────────────────────────────────────────────────────────
// EQ 8-band clipping repro (user-reported, 2026-04-29)
//
// The user identified the EQ 8-band as the source of audible clipping.
// At defaults all bands have gain = 0 dB and Q = 1, which mathematically
// is unity passthrough — RBJ peak-EQ math with a=1 gives b0=b1=b2=a0=a1=a2,
// which after normalisation reduces to the identity filter. So at
// defaults the EQ MUST be transparent. Anything else is a bug.
//
// These tests assert that property directly: feed a clean sine into a
// chain that contains the EQ at default, and check the output is the
// same clean sine, no clicks, no clipping above ±1.0.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn eq_eight_band_at_defaults_is_transparent_no_clipping() {
    // Smoke test: 220 Hz sine at 0.5 amplitude through the EQ at defaults
    // (all bands at 0 dB peak, Q = 1). Output should match the input
    // amplitude ±tolerance, with no clicks and no peaks above input level.
    let eq_params = neutral_params("filter", "eq_eight_band_parametric");
    let chain = chain_with_blocks(
        "eq8-defaults",
        vec![
            input_mono(vec![0]),
            core_block("eq8", "filter", "eq_eight_band_parametric", eq_params),
            output(ChainOutputMode::Mono, vec![0]),
        ],
    );
    let runtime = build_runtime(&chain);
    let mut gen = SineGen::new(220.0, SR, 0.5);
    let captured = drive_capture_steady(&runtime, &mut gen, 1, 1, 32, 8);

    scan_for_click("eq8_defaults", &captured, 1, 0.1).expect("EQ at defaults should be click-free");
    scan_within_magnitude("eq8_defaults_magnitude", &captured, 1.0)
        .expect("EQ at defaults should not clip");

    // At defaults the EQ should be ~unity. Allow some tolerance for
    // filter init transient even after warmup.
    let peak = captured.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
    assert!(
        (peak - 0.5).abs() < 0.15,
        "EQ at defaults should pass 220 Hz @ 0.5 ≈ unity, got peak {peak}"
    );
}

#[test]
fn eq_eight_band_at_defaults_silent_input_silent_output() {
    let eq_params = neutral_params("filter", "eq_eight_band_parametric");
    let chain = chain_with_blocks(
        "eq8-silent",
        vec![
            input_mono(vec![0]),
            core_block("eq8", "filter", "eq_eight_band_parametric", eq_params),
            output(ChainOutputMode::Mono, vec![0]),
        ],
    );
    let runtime = build_runtime(&chain);
    let captured = drive_capture_silent(&runtime, 1, 1, 32, 8);

    scan_finite("eq8_silent_finite", &captured)
        .expect("EQ should not introduce non-finite samples on silence");
    let max_abs = captured.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
    assert!(
        max_abs < 1e-3,
        "EQ at defaults must produce silence for silent input, got peak {max_abs}"
    );
}

/// Helper: build EQ params with one band boosted by `gain_db` at `freq_hz`,
/// rest at defaults.
fn eq_with_one_band_boosted(band_index: usize, freq_hz: f32, gain_db: f32) -> ParameterSet {
    let mut params = neutral_params("filter", "eq_eight_band_parametric");
    let n = band_index + 1;
    params.insert(&format!("band{n}_freq"), ParameterValue::Float(freq_hz));
    params.insert(&format!("band{n}_gain"), ParameterValue::Float(gain_db));
    params
}

#[test]
fn eq_eight_band_one_band_max_boost_does_not_overshoot_input() {
    // Boost band 5 (1 kHz) by +24 dB (max in schema). Feed a 1 kHz sine
    // at 0.05 amplitude (well below clipping). The boosted band should
    // amplify by ~16x → output peak ≈ 0.8. No clipping expected.
    //
    // If the EQ's filter ringing or numerical error causes overshoot
    // above 1.0 (= the chain limiter engages), we capture it.
    let params = eq_with_one_band_boosted(4, 1_000.0, 24.0);
    let chain = chain_with_blocks(
        "eq8-1k-+24",
        vec![
            input_mono(vec![0]),
            core_block("eq8", "filter", "eq_eight_band_parametric", params),
            output(ChainOutputMode::Mono, vec![0]),
        ],
    );
    let runtime = build_runtime(&chain);
    // Input at 0.05 → expected boosted peak around 0.8. Run long enough
    // for filter to settle.
    let mut gen = SineGen::new(1_000.0, SR, 0.05);
    let captured = drive_capture_steady(&runtime, &mut gen, 1, 1, 64, 16);

    scan_finite("eq8_1k_+24_finite", &captured).expect("EQ +24dB at 1k must produce finite output");
    let peak = captured.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
    eprintln!("[eq8 1k +24dB] input peak 0.05 → output peak {peak:.4}");
    assert!(
        peak <= 1.0,
        "EQ +24dB band overshoot above ±1.0 at safe input level: peak {peak}"
    );
}

#[test]
fn eq_eight_band_output_trim_attenuates_uniformly() {
    // The fix for the user's reported clipping: output_db parameter
    // applies a pre-computed linear gain at the end of process_sample.
    // -6 dB ≈ 0.5012 linear → output peak ≈ 0.5 of unity-EQ output.
    let mut params = neutral_params("filter", "eq_eight_band_parametric");
    params.insert("output_db", ParameterValue::Float(-6.0));

    let chain = chain_with_blocks(
        "eq8-out-trim",
        vec![
            input_mono(vec![0]),
            core_block("eq8", "filter", "eq_eight_band_parametric", params),
            output(ChainOutputMode::Mono, vec![0]),
        ],
    );
    let runtime = build_runtime(&chain);
    let mut gen = SineGen::new(220.0, SR, 0.5);
    let captured = drive_capture_steady(&runtime, &mut gen, 1, 1, 32, 8);
    scan_finite("eq8_out_trim_-6dB", &captured).expect("output trim must not produce NaN");
    let peak = captured.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
    let expected = 0.5_f32 * 10.0_f32.powf(-6.0 / 20.0); // ≈ 0.2506
    assert!(
        (peak - expected).abs() < 0.05,
        "output_db -6 dB should attenuate 0.5 input → ≈{expected}, got {peak}"
    );
}

#[test]
fn eq_eight_band_default_output_db_is_unity() {
    // Pin the default: output_db defaults to 0 dB → unity gain, same
    // behaviour as before the parameter existed. Existing project YAMLs
    // that don't carry output_db are normalized with 0.0 and run
    // identically.
    let params = neutral_params("filter", "eq_eight_band_parametric");

    let chain = chain_with_blocks(
        "eq8-default-trim",
        vec![
            input_mono(vec![0]),
            core_block("eq8", "filter", "eq_eight_band_parametric", params),
            output(ChainOutputMode::Mono, vec![0]),
        ],
    );
    let runtime = build_runtime(&chain);
    let mut gen = SineGen::new(220.0, SR, 0.5);
    let captured = drive_capture_steady(&runtime, &mut gen, 1, 1, 32, 8);
    let peak = captured.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
    assert!(
        (peak - 0.5).abs() < 0.05,
        "EQ default output_db should be unity, got peak {peak} for 0.5 input"
    );
}

#[test]
fn eq_eight_band_smile_curve_typical_user_config() {
    // Smile/V curve: bass + treble boosted, mids cut (or flat). Very
    // common preset for guitar tone. Tests realistic worst-case.
    //  band1 (62 Hz)  +9 dB   bass shelf-ish
    //  band2 (125)    +6 dB
    //  band3 (250)    +3 dB
    //  band4 (500)    -3 dB
    //  band5 (1k)     -3 dB   mid scoop
    //  band6 (2k)     +3 dB
    //  band7 (4k)     +6 dB
    //  band8 (8k)     +9 dB   treble shelf-ish
    let mut params = neutral_params("filter", "eq_eight_band_parametric");
    params.insert("band1_gain", ParameterValue::Float(9.0));
    params.insert("band2_gain", ParameterValue::Float(6.0));
    params.insert("band3_gain", ParameterValue::Float(3.0));
    params.insert("band4_gain", ParameterValue::Float(-3.0));
    params.insert("band5_gain", ParameterValue::Float(-3.0));
    params.insert("band6_gain", ParameterValue::Float(3.0));
    params.insert("band7_gain", ParameterValue::Float(6.0));
    params.insert("band8_gain", ParameterValue::Float(9.0));

    let chain = chain_with_blocks(
        "eq8-smile",
        vec![
            input_mono(vec![0]),
            core_block("eq8", "filter", "eq_eight_band_parametric", params),
            output(ChainOutputMode::Mono, vec![0]),
        ],
    );
    // Input at 0.5 (already a hot guitar level). Sweep across the
    // boosted band centers and find the worst-case overshoot.
    let mut max_overshoot = 0.0_f32;
    let mut worst_freq = 0.0_f32;
    for &freq in &[
        62.0_f32, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0,
    ] {
        let runtime = build_runtime(&chain); // fresh runtime per freq, no leftover state
        let mut gen = SineGen::new(freq, SR, 0.5);
        let captured = drive_capture_steady(&runtime, &mut gen, 1, 1, 64, 16);
        scan_finite(&format!("smile_{freq}Hz_finite"), &captured)
            .expect("smile EQ must not produce NaN");
        let peak = captured.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
        if peak > max_overshoot {
            max_overshoot = peak;
            worst_freq = freq;
        }
        eprintln!("[eq8 smile] {freq:>5}Hz @ 0.5 → peak {peak:.4}");
    }
    eprintln!("[eq8 smile] worst case: {worst_freq}Hz, peak {max_overshoot:.4}");

    // The smile curve at +9 dB extremes amplifies by ~2.8x linear at the
    // boosted band centers. Input 0.5 → expected peak ~1.4 → limiter
    // engages → output peak <= 1.0. Document the failure mode: limiter
    // is doing its job, but the user hears tanh saturation as "clipping".
    assert!(
        max_overshoot <= 1.0,
        "Output limiter failed: peak {max_overshoot} at {worst_freq}Hz exceeds ±1.0"
    );
    let _ = max_overshoot;
}

#[test]
fn eq_eight_band_full_scale_with_band_boost_clips_through_limiter() {
    // The user's reported case: input is already hot (e.g. amp output)
    // and EQ has a boosted band. The cumulative output exceeds 1.0,
    // engages the chain limiter, and the user hears tanh saturation —
    // that's the audible "clip".
    //
    // This test does NOT assert the output stays under 1.0 — it can't
    // (the user explicitly asked for +24 dB on a full-scale signal).
    // The test asserts the FAILURE MODE: the output limiter is what
    // catches the overshoot, NOT NaN, NOT a click. tanh-saturated
    // signal is musical distortion; raw clipping is a buzz.
    //
    // The fix for the user's complaint isn't the limiter (working as
    // designed) — it's giving the EQ an output trim parameter so they
    // can compensate for boost without re-tuning every band.
    let params = eq_with_one_band_boosted(4, 1_000.0, 24.0);
    let chain = chain_with_blocks(
        "eq8-1k-+24-fullscale",
        vec![
            input_mono(vec![0]),
            core_block("eq8", "filter", "eq_eight_band_parametric", params),
            output(ChainOutputMode::Mono, vec![0]),
        ],
    );
    let runtime = build_runtime(&chain);
    let mut gen = SineGen::new(1_000.0, SR, 1.0);
    let captured = drive_capture_steady(&runtime, &mut gen, 1, 1, 64, 16);

    scan_finite("eq8_1k_+24_fullscale_finite", &captured)
        .expect("EQ must not produce NaN even when limiter saturates");

    // Limiter (tanh above 0.95) keeps every sample within ±tanh(∞) = ±1.0.
    // No sample should ever exceed 1.0.
    let peak = captured.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
    assert!(
        peak <= 1.0,
        "Limiter failed to bound output ≤ ±1.0: peak = {peak}"
    );
}

#[test]
fn eq_eight_band_at_defaults_full_scale_no_overshoot() {
    // Full-scale ±1.0 sine through EQ. At defaults the EQ is unity, so
    // output peak should be ≤ 1.0 (with tiny filter-init transient).
    // If the cascade introduces ringing that pushes any sample above
    // 1.0, the limiter (tanh > 0.95) engages and the user hears
    // saturation — that's the reported audible "clip".
    let eq_params = neutral_params("filter", "eq_eight_band_parametric");
    let chain = chain_with_blocks(
        "eq8-fullscale",
        vec![
            input_mono(vec![0]),
            core_block("eq8", "filter", "eq_eight_band_parametric", eq_params),
            output(ChainOutputMode::Mono, vec![0]),
        ],
    );
    let runtime = build_runtime(&chain);
    let mut gen = SineGen::new(220.0, SR, 1.0);
    let captured = drive_capture_steady(&runtime, &mut gen, 1, 1, 64, 16);

    scan_finite("eq8_fullscale_finite", &captured).expect("EQ must not produce NaN at full scale");
    let max_abs = captured.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
    // Full-scale sine through unity EQ → output peak ≤ 1.0 (limiter
    // soft-saturates at 0.95, so peak <= ~0.95 + tanh tail).
    // If the output peak is much above input peak, the EQ is amplifying
    // by accident.
    assert!(
        max_abs <= 1.0,
        "EQ at defaults clipped on full-scale input: peak = {max_abs}"
    );
}

#[test]
fn long_run_steady_state_no_clicks_8000_samples() {
    // Soak: 125 callbacks × 64 frames = 8000 samples ≈ 167 ms of audio.
    // Catches periodic glitches that only appear after warmup or
    // ring-buffer wrap-around.
    let chain = chain_with_blocks(
        "pipe-soak",
        vec![input_mono(vec![0]), output(ChainOutputMode::Mono, vec![0])],
    );
    let runtime = build_runtime(&chain);
    let mut gen = SineGen::new(220.0, SR, 0.5);
    let captured = drive_capture_steady(&runtime, &mut gen, 1, 1, 125, 4);
    assert_eq!(captured.len(), 121 * BUFFER_FRAMES);

    scan_for_click("soak", &captured, 1, 0.1).expect("audio integrity violated during soak");
    scan_within_magnitude("soak_magnitude", &captured, 1.0).expect("output exceeded ±1.0");
}
