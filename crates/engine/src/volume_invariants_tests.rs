//! Volume non-regression contract tests (issue #355, CLAUDE.md
//! invariant #10).
//!
//! THE RULE: nothing in this engine — refactor, fix, performance work,
//! cleanup, split — may alter per-stream volume without an explicit
//! user request. Solo guitar in any chain comes out at unity. Two
//! guitars summing to clipping is the output limiter's job, not a
//! preemptive 1/N scale.
//!
//! These tests are the authoritative pin. If you break them, the
//! source is wrong, not the tests. Adjust the source until the tests
//! pass; never relax the assertions.

use super::{
    build_chain_runtime_state, process_input_f32, process_output_f32, AudioFrame,
    DEFAULT_ELASTIC_TARGET,
};
use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use std::sync::Arc;

const SR: f32 = 48_000.0;
const TOLERANCE: f32 = 1e-3;

// ─────────────────────────────────────────────────────────────────────────
// Chain builders
// ─────────────────────────────────────────────────────────────────────────

/// Single-input mono chain (the user's reported Mac setup):
///   `mode: mono, channels: [0]` → stereo output `[0, 1]`.
fn single_mono_input_chain() -> Chain {
    Chain {
        id: ChainId("single_mono".into()),
        description: Some("single mono guitar".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        blocks: vec![
            AudioBlock {
                id: BlockId("single_mono:input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
            AudioBlock {
                id: BlockId("single_mono:output:0".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    }
}

/// Split-mono chain: ONE InputBlock with `mode: mono, channels: [0, 1]`.
/// This is the case #350 originally addressed by introducing 1/N. The
/// new contract: streams contribute at unity, regardless of how many
/// siblings the InputBlock declares.
fn split_mono_input_chain() -> Chain {
    Chain {
        id: ChainId("split_mono".into()),
        description: Some("two guitars on one mono input".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        blocks: vec![
            AudioBlock {
                id: BlockId("split_mono:input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0, 1],
                    }],
                }),
            },
            AudioBlock {
                id: BlockId("split_mono:output:0".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────

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

fn const_interleaved(per_channel: &[f32], frames: usize) -> Vec<f32> {
    let mut buf = Vec::with_capacity(per_channel.len() * frames);
    for _ in 0..frames {
        for &v in per_channel {
            buf.push(v);
        }
    }
    buf
}

/// Drive `frames` frames using `data` per callback chunk, return the
/// peak across the steady-state captures (skipping the first two
/// callbacks to drop the FADE_IN ramp).
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

// ─────────────────────────────────────────────────────────────────────────
// Block A — Single input never sees attenuation
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn single_mono_input_passes_through_at_unity_gain() {
    let chain = single_mono_input_chain();
    let peak = measure_steady_peak(&chain, 1, &[0.5], 2, 4);
    assert!(
        (peak - 0.5).abs() < TOLERANCE,
        "single mono input must emit at unity gain; expected ≈ 0.5, got {peak}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Block B — Split-mono solo: active stream at unity, no penalty
// ─────────────────────────────────────────────────────────────────────────

/// CONTRACT (CLAUDE.md invariant #10): in a split-mono chain
/// (`mode: mono, channels: [0, 1]`), when only ONE channel carries
/// signal and the other is silent, the active stream MUST come out at
/// unity gain. The previous `1/N` preemptive attenuation has been
/// removed — solo playback pays no penalty just because the slot is
/// configured.
#[test]
fn split_mono_with_one_active_stream_emits_at_unity_gain() {
    let chain = split_mono_input_chain();
    let peak = measure_steady_peak(&chain, 2, &[0.5, 0.0], 2, 4);
    assert!(
        (peak - 0.5).abs() < TOLERANCE,
        "split-mono solo must emit active stream at unity gain; expected ≈ 0.5, got {peak}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Block C — Split-mono dual: each stream at unity, limiter holds the sum
// ─────────────────────────────────────────────────────────────────────────

/// CONTRACT (CLAUDE.md invariant #10): when both split-mono channels
/// carry signal, EACH stream still contributes at unity gain. The sum
/// of two unity contributions can exceed 0 dBFS — that is the output
/// limiter's job to handle (tanh above 0.95). NO preemptive 1/N scale.
///
/// At per-channel amplitude 0.3 (sum 0.6), the limiter is transparent
/// (below 0.95) and the output equals the sum.
#[test]
fn split_mono_dual_active_emits_unity_per_stream_below_limiter_knee() {
    let chain = split_mono_input_chain();
    let peak = measure_steady_peak(&chain, 2, &[0.3, 0.3], 2, 4);
    assert!(
        (peak - 0.6).abs() < TOLERANCE,
        "split-mono dual below limiter knee must sum at unity; expected ≈ 0.6, got {peak}"
    );
}

/// CONTRACT (CLAUDE.md invariant #10): when split-mono dual sums above
/// the limiter knee, the output is `tanh(sum)` — gentle saturation by
/// the existing `output_limiter`, NOT a preemptive 1/N scale.
///
/// At per-channel 0.8 (sum 1.6), tanh(1.6) ≈ 0.9217. The previous 1/N
/// fix (#350) was returning ≈ 0.8 here — a different value that had
/// the side-effect of also halving the solo case. We trade that side-
/// effect for limiter saturation when both are loud, which is the
/// physically correct behavior.
#[test]
fn split_mono_dual_active_above_limiter_knee_uses_tanh_limiter() {
    let chain = split_mono_input_chain();
    let peak = measure_steady_peak(&chain, 2, &[0.8, 0.8], 2, 4);
    let expected = (1.6_f32).tanh();
    assert!(
        (peak - expected).abs() < 0.01,
        "split-mono dual above knee must equal tanh(sum); expected ≈ {expected}, got {peak}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Block D — Mono → Stereo broadcast preserved (CLAUDE.md invariant #5)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn mono_input_broadcasts_to_both_output_channels() {
    let chain = single_mono_input_chain();
    let runtime = build_runtime(&chain);
    let warmup = const_interleaved(&[0.4], 512);
    drive_and_capture(&runtime, 1, &warmup, 2);
    let data = const_interleaved(&[0.4], 256);
    let out = drive_and_capture(&runtime, 1, &data, 2);
    let lefts: Vec<f32> = out.iter().step_by(2).copied().collect();
    let rights: Vec<f32> = out.iter().skip(1).step_by(2).copied().collect();
    let l_peak = peak_abs(&lefts);
    let r_peak = peak_abs(&rights);
    assert!(
        (l_peak - r_peak).abs() < TOLERANCE,
        "mono → stereo broadcast must put signal in BOTH channels; L peak {l_peak}, R peak {r_peak}"
    );
    assert!(l_peak > 0.3, "left channel must carry signal; got {l_peak}");
    assert!(
        r_peak > 0.3,
        "right channel must carry signal; got {r_peak}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Block I — Anti-revert structural pin
// ─────────────────────────────────────────────────────────────────────────

/// CONTRACT (CLAUDE.md invariant #10): the per-segment split_scale at
/// fan-out time MUST be 1.0 — no preemptive attenuation for any reason.
/// If you need to add an auto-mix feature, it is an opt-in toggle in
/// the UI, gated by an explicit user request, NOT a default behaviour.
///
/// This test detects any reintroduction of `1.0 / N` (or any other
/// preemptive scale): solo playback in a split-mono chain MUST equal
/// solo playback in a single-mono chain at the same input level.
#[test]
fn split_mono_solo_volume_equals_single_mono_volume() {
    let split = split_mono_input_chain();
    let single = single_mono_input_chain();
    let split_peak = measure_steady_peak(&split, 2, &[0.5, 0.0], 2, 4);
    let single_peak = measure_steady_peak(&single, 1, &[0.5], 2, 4);
    assert!(
        (split_peak - single_peak).abs() < TOLERANCE,
        "split-mono solo and single-mono must emit at the SAME level; \
         split peak = {split_peak}, single peak = {single_peak}. \
         A drift here means a preemptive scale was reintroduced — \
         search for `split_scale` in runtime.rs and remove the attenuation."
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Block J — User reproducer (Mac, single mono)
// ─────────────────────────────────────────────────────────────────────────

/// REGRESSION DOC: this test replicates the YAML the user reported on
/// 2026-04-28 ("som muito mais baixo no Mac"): one InputBlock,
/// `mode: mono, channels: [0]`. If this test ever fails, the volume
/// drop is back — open an investigation immediately.
#[test]
fn user_reported_mac_volume_drop_does_not_recur() {
    let chain = single_mono_input_chain();
    let peak = measure_steady_peak(&chain, 1, &[0.3], 2, 8);
    assert!(
        (peak - 0.3).abs() < TOLERANCE,
        "Mac single-mono setup must hold steady at unity gain; expected ≈ 0.3, got {peak}"
    );
}

#[allow(dead_code)]
fn _suppress_audio_frame_dead_code(f: AudioFrame) -> AudioFrame {
    f
}
