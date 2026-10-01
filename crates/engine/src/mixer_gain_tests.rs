//! Global mixer gain on the live audio path (issue #1007).
//!
//! Every test uses its OWN device id: the endpoint table is process-global
//! and tests run in parallel.

use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::mixer_gain::strip_linear_gain;
use domain::mixer_strip::MixerDirection;
use project::chain::Chain;

use crate::mixer_gains::{endpoint_gain_target, set_endpoint_gain};
use crate::runtime::{
    build_chain_runtime_state, process_input_f32, process_output_f32, ChainRuntimeState,
    DEFAULT_ELASTIC_TARGET,
};

const SR: f32 = 48_000.0;
const FRAMES: usize = 256;
const LEVEL: f32 = 0.5;

fn chain_on(device: &str) -> (Chain, Vec<IoBinding>) {
    let registry = vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "Guitar".into(),
            device_id: DeviceId(device.into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Main".into(),
            device_id: DeviceId(device.into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }];
    let chain = Chain {
        disabled_endpoints: Default::default(),
        id: ChainId(format!("chain-{device}")),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        mix: Default::default(),
    };
    (chain, registry)
}

fn runtime_on(device: &str) -> Arc<ChainRuntimeState> {
    let (chain, registry) = chain_on(device);
    Arc::new(
        build_chain_runtime_state(&chain, SR, &[DEFAULT_ELASTIC_TARGET], &registry)
            .expect("runtime builds"),
    )
}

/// One input callback of constant LEVEL on one channel, then one output
/// callback on two channels.
fn callback(runtime: &Arc<ChainRuntimeState>) -> Vec<f32> {
    let input = vec![LEVEL; FRAMES];
    process_input_f32(runtime, 0, &input, 1);
    let mut out = vec![0.0_f32; FRAMES * 2];
    process_output_f32(runtime, 0, &mut out, 2);
    out
}

/// Run past the rebuild fade-in so every later sample is steady state.
fn settle(runtime: &Arc<ChainRuntimeState>) {
    for _ in 0..4 {
        callback(runtime);
    }
}

fn db(gain_db: f32) -> f32 {
    strip_linear_gain(gain_db, false)
}

fn assert_all(out: &[f32], expected: f32, tol: f32, what: &str) {
    for (i, s) in out.iter().enumerate() {
        assert!(
            (s - expected).abs() <= tol,
            "{what}: sample {i} = {s}, expected {expected}"
        );
    }
}

#[test]
fn untouched_endpoint_is_bit_identical() {
    let rt = runtime_on("mx-untouched");
    settle(&rt);
    let out = callback(&rt);
    assert!(
        out.iter().all(|&s| s == LEVEL),
        "unity must not touch a bit"
    );
}

#[test]
fn output_fader_scales_its_output() {
    set_endpoint_gain(MixerDirection::Output, "mx-out", &[0, 1], db(-6.0));
    let rt = runtime_on("mx-out");
    settle(&rt);
    assert_all(&callback(&rt), LEVEL * db(-6.0), 1e-5, "output -6 dB");
}

#[test]
fn input_fader_scales_its_input() {
    set_endpoint_gain(MixerDirection::Input, "mx-in", &[0], db(-6.0));
    let rt = runtime_on("mx-in");
    settle(&rt);
    assert_all(&callback(&rt), LEVEL * db(-6.0), 1e-5, "input -6 dB");
}

#[test]
fn output_mute_silences_its_output() {
    set_endpoint_gain(MixerDirection::Output, "mx-mute", &[0, 1], 0.0);
    let rt = runtime_on("mx-mute");
    settle(&rt);
    assert_all(&callback(&rt), 0.0, 0.0, "muted output");
}

#[test]
fn a_fader_move_reaches_a_live_runtime_without_a_step() {
    let rt = runtime_on("mx-live");
    settle(&rt);
    set_endpoint_gain(MixerDirection::Output, "mx-live", &[0, 1], db(-6.0));
    let glide = callback(&rt);
    let target = LEVEL * db(-6.0);
    // Stereo interleaved: compare frame to frame on the left channel.
    let left: Vec<f32> = glide.iter().step_by(2).copied().collect();
    let max_step = left
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        (left[0] - LEVEL).abs() < 0.01,
        "first sample must continue from unity, got {}",
        left[0]
    );
    assert!(max_step < 0.002, "ramp jumps {max_step} between frames");
    assert!(
        (left[left.len() - 1] - target).abs() < 1e-5,
        "ramp must land on the target inside the callback, got {}",
        left[left.len() - 1]
    );
    assert_all(&callback(&rt), target, 1e-5, "after the glide");
}

#[test]
fn back_to_unity_is_bit_identical_again() {
    let rt = runtime_on("mx-back");
    settle(&rt);
    set_endpoint_gain(MixerDirection::Output, "mx-back", &[0, 1], db(-6.0));
    callback(&rt);
    set_endpoint_gain(MixerDirection::Output, "mx-back", &[0, 1], 1.0);
    callback(&rt);
    let out = callback(&rt);
    assert!(out.iter().all(|&s| s == LEVEL), "unity again must be exact");
}

#[test]
fn a_fader_only_touches_its_own_endpoint() {
    let a = runtime_on("mx-iso-a");
    let b = runtime_on("mx-iso-b");
    settle(&a);
    settle(&b);
    set_endpoint_gain(MixerDirection::Output, "mx-iso-a", &[0, 1], db(-12.0));
    set_endpoint_gain(MixerDirection::Input, "mx-iso-a", &[0], db(-12.0));
    // Glide, then one more callback for the route cushion to drain.
    for _ in 0..2 {
        callback(&a);
        callback(&b);
    }
    assert_all(&callback(&a), LEVEL * db(-24.0), 1e-5, "stream A");
    let out_b = callback(&b);
    assert!(out_b.iter().all(|&s| s == LEVEL), "stream B must not move");
    assert_eq!(
        endpoint_gain_target(MixerDirection::Output, "mx-iso-b", &[0, 1]),
        1.0
    );
}
