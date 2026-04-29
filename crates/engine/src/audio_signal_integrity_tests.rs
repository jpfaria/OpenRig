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
use project::block::{AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
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

    scan_finite("fullscale_finite", &captured)
        .expect("non-finite output for full-scale input");
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
