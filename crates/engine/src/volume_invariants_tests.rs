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

pub(super) use super::{
    build_chain_runtime_state, process_input_f32, process_output_f32, DEFAULT_ELASTIC_TARGET,
};
pub(super) use domain::ids::{BlockId, ChainId, DeviceId};
pub(super) use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
pub(super) use domain::value_objects::ParameterValue;
pub(super) use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
pub(super) use project::chain::Chain;
pub(super) use project::param::ParameterSet;
pub(super) use std::sync::Arc;

pub(super) const SR: f32 = 48_000.0;
pub(super) const TOLERANCE: f32 = 1e-3;

// ─────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────

/// Registry id every chain in this file references via `io_binding_ids`.
pub(super) const IO_BINDING_ID: &str = "io";

// The chain's physical I/O lives in the per-machine registry now (model A).
// These helpers return the registry endpoint describing one input / output;
// device / mode / channels are preserved exactly from the old Input/Output
// blocks — only the SET-UP form changed.

pub(super) fn input_mono(channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: "in0".into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels,
    }
}

pub(super) fn input_stereo(channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: "in0".into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels,
    }
}

pub(super) fn input_dual_mono(channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: "in0".into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::DualMono,
        channels,
    }
}

pub(super) fn output(mode: ChannelMode, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: "out0".into(),
        device_id: DeviceId("dev".into()),
        mode,
        channels,
    }
}

pub(super) fn neutral_params(effect_type: &str, model: &str) -> ParameterSet {
    let schema =
        schema_for_block_model(effect_type, model).expect("schema must exist for test model");
    ParameterSet::default()
        .normalized_against(&schema)
        .expect("defaults must normalize")
}

pub(super) fn core_block(id: &str, effect_type: &str, model: &str, params: ParameterSet) -> AudioBlock {
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

pub(super) fn chain_with_blocks(
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
        di_output: None,
    };
    (chain, registry)
}

pub(super) fn build_runtime(chain: &Chain, registry: &[IoBinding]) -> Arc<super::ChainRuntimeState> {
    Arc::new(
        build_chain_runtime_state(chain, SR, &[DEFAULT_ELASTIC_TARGET], registry)
            .expect("runtime state should build"),
    )
}

pub(super) fn drive_and_capture(
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

pub(super) fn peak_abs(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |a, &b| a.max(b.abs()))
}

pub(super) fn min_abs(samples: &[f32]) -> f32 {
    samples.iter().fold(f32::INFINITY, |a, &b| a.min(b.abs()))
}

pub(super) fn const_interleaved(per_channel: &[f32], frames: usize) -> Vec<f32> {
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
pub(super) fn measure_steady_peak(
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
pub(super) fn measure_steady_per_channel_peak(
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

pub(super) fn bare_chain_for(id: &str) -> (Chain, Vec<IoBinding>) {
    chain_with_blocks(
        id,
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    )
}

pub(super) fn pink_noise(n: usize, seed: u64) -> Vec<f32> {
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
