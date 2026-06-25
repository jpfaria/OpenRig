//! Stereo-image invariant guard (#485 / CLAUDE.md invariant #5).
//!
//! "Stream é SEMPRE estéreo internamente. Mono input → broadcast
//! Stereo([s,s])." A mono guitar through a *stereo* effect (MonoToStereo
//! reverb/chorus) MUST reach the output with L != R; a chain with no
//! stereo block is correctly dual-mono (L == R, both channels non-silent).
//! These tests pin that end-to-end so the dual-mono regression can never
//! come back silently.

use super::{
    build_chain_runtime_state, process_input_f32, process_output_f32, DEFAULT_ELASTIC_TARGET,
};
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::value_objects::ParameterValue;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use std::sync::Arc;

const SR: f32 = 48_000.0;
const BUFFER_FRAMES: usize = 64;

/// Registry mirroring the legacy mono-in (ch0) / stereo-out (ch0,1) endpoints.
/// The chain resolves head input and tail output from this binding (#716).
fn registry() -> Vec<domain::io_binding::IoBinding> {
    vec![domain::io_binding::IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![domain::io_binding::IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![domain::io_binding::IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn chain_with_blocks(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some("stereo image test".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks,
    }
}

fn params_with(effect_type: &str, model: &str, overrides: &[(&str, f32)]) -> ParameterSet {
    let schema =
        schema_for_block_model(effect_type, model).expect("schema must exist for test model");
    let mut p = ParameterSet::default()
        .normalized_against(&schema)
        .expect("defaults must normalize");
    for (k, v) in overrides {
        p.insert(*k, ParameterValue::Float(*v));
    }
    p
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

fn build(chain: &Chain) -> Arc<super::ChainRuntimeState> {
    Arc::new(
        build_chain_runtime_state(chain, SR, &[DEFAULT_ELASTIC_TARGET], &registry())
            .expect("runtime state should build"),
    )
}

/// Drive a 1 kHz-ish mono ramp/sine through `n` callbacks, return the
/// concatenated interleaved stereo output of the steady-state callbacks.
fn drive_stereo(runtime: &Arc<super::ChainRuntimeState>, n: usize, warmup: usize) -> Vec<f32> {
    let mut input_buf = vec![0.0_f32; BUFFER_FRAMES];
    let mut output_buf = vec![0.0_f32; BUFFER_FRAMES * 2];
    let mut captured = Vec::with_capacity((n - warmup) * BUFFER_FRAMES * 2);
    let mut phase = 0.0_f32;
    let step = 2.0 * std::f32::consts::PI * 440.0 / SR;
    for cb in 0..n {
        for s in input_buf.iter_mut() {
            *s = 0.5 * phase.sin();
            phase += step;
        }
        process_input_f32(runtime, 0, &input_buf, 1);
        process_output_f32(runtime, 0, &mut output_buf, 2);
        if cb >= warmup {
            captured.extend_from_slice(&output_buf);
        }
    }
    captured
}

fn max_lr_divergence(interleaved_stereo: &[f32]) -> f32 {
    interleaved_stereo
        .chunks_exact(2)
        .map(|f| (f[0] - f[1]).abs())
        .fold(0.0_f32, f32::max)
}

fn peak_abs(interleaved_stereo: &[f32]) -> f32 {
    interleaved_stereo
        .iter()
        .fold(0.0_f32, |m, s| m.max(s.abs()))
}

#[test]
fn mono_in_stereo_block_stereo_out_is_true_stereo() {
    // Mono guitar → MonoToStereo reverb (wet) → stereo output.
    // Invariant #5: the stereo effect MUST reach the output decorrelated
    // (L != R). If this fails, the pipeline is collapsing stereo to mono.
    let chain = chain_with_blocks(
        "mono-reverb-stereo",
        vec![core_block(
            "rv",
            "reverb",
            "room",
            params_with("reverb", "room", &[("mix", 60.0), ("room_size", 70.0)]),
        )],
    );
    let runtime = build(&chain);
    // Reverb needs tail build-up; warm up generously.
    let out = drive_stereo(&runtime, 96, 32);

    assert!(
        peak_abs(&out) > 1e-3,
        "output is silent — chain not producing"
    );
    let div = max_lr_divergence(&out);
    assert!(
        div > 1e-3,
        "STEREO COLLAPSE: mono input through a MonoToStereo reverb \
         produced L == R (max |L-R| = {div:e}); invariant #5 violated"
    );
}

#[test]
fn mono_in_stereo_block_via_modulation_is_true_stereo() {
    let chain = chain_with_blocks(
        "mono-chorus-stereo",
        vec![core_block(
            "ch",
            "modulation",
            "stereo_chorus",
            params_with("modulation", "stereo_chorus", &[("mix", 60.0)]),
        )],
    );
    let runtime = build(&chain);
    let out = drive_stereo(&runtime, 96, 32);

    assert!(peak_abs(&out) > 1e-3, "output is silent");
    let div = max_lr_divergence(&out);
    assert!(
        div > 1e-4,
        "STEREO COLLAPSE: stereo_chorus produced L == R (max |L-R| = {div:e})"
    );
}

#[test]
fn mono_in_no_stereo_block_is_dual_mono_not_silent() {
    // No stereo block: dual-mono (L == R) is the CORRECT behaviour for a
    // mono source (invariant #5 broadcast). Pins that the broadcast keeps
    // both channels non-silent and identical (not "one channel only").
    let chain = chain_with_blocks(
        "mono-pipe-stereo-out",
        vec![core_block(
            "vol",
            "gain",
            "volume",
            params_with("gain", "volume", &[("volume", 80.0)]),
        )],
    );
    let runtime = build(&chain);
    let out = drive_stereo(&runtime, 48, 16);

    assert!(peak_abs(&out) > 1e-3, "broadcast produced silence");
    let div = max_lr_divergence(&out);
    assert!(
        div < 1e-6,
        "expected dual-mono (L == R) with no stereo block, got max |L-R| = {div:e}"
    );
}
