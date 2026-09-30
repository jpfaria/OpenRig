//! A chain's own mixer faders on the live audio path (issue #1007).
//!
//! The chain-local fader of an endpoint multiplies with that endpoint's
//! global fader and touches only its own chain. Every test uses its OWN
//! device id: the endpoint tables are process-global and tests run in
//! parallel.

use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::mixer_gain::strip_linear_gain;
use domain::mixer_strip::MixerDirection;
use project::chain::Chain;

use crate::chain_mix_gains::apply_chain_mix;
use crate::di_loop::DiLoop;
use crate::mixer_gains::set_endpoint_gain;
use crate::runtime::{
    build_chain_runtime_state, process_input_f32, process_output_f32, ChainRuntimeState,
    DEFAULT_ELASTIC_TARGET,
};

const SR: f32 = 48_000.0;
const FRAMES: usize = 256;
const LEVEL: f32 = 0.5;

fn registry_on(device: &str) -> Vec<IoBinding> {
    vec![IoBinding {
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
    }]
}

fn chain_named(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        mix: Default::default(),
    }
}

fn runtime(chain: &Chain, registry: &[IoBinding]) -> Arc<ChainRuntimeState> {
    Arc::new(
        build_chain_runtime_state(chain, SR, &[DEFAULT_ELASTIC_TARGET], registry)
            .expect("runtime builds"),
    )
}

fn callback(runtime: &Arc<ChainRuntimeState>) -> Vec<f32> {
    let input = vec![LEVEL; FRAMES];
    process_input_f32(runtime, 0, &input, 1);
    let mut out = vec![0.0_f32; FRAMES * 2];
    process_output_f32(runtime, 0, &mut out, 2);
    out
}

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
fn chain_input_fader_scales_its_input() {
    let registry = registry_on("cm-in");
    let mut chain = chain_named("cm-in-chain");
    chain
        .mix
        .endpoint_mut(MixerDirection::Input, "io", "Guitar")
        .gain_db = -6.0;
    let rt = runtime(&chain, &registry);
    settle(&rt);
    assert_all(&callback(&rt), LEVEL * db(-6.0), 1e-5, "chain input -6 dB");
}

#[test]
fn chain_output_fader_scales_its_output() {
    let registry = registry_on("cm-out");
    let mut chain = chain_named("cm-out-chain");
    chain
        .mix
        .endpoint_mut(MixerDirection::Output, "io", "Main")
        .gain_db = -6.0;
    let rt = runtime(&chain, &registry);
    settle(&rt);
    assert_all(&callback(&rt), LEVEL * db(-6.0), 1e-5, "chain output -6 dB");
}

#[test]
fn chain_mute_silences_that_chain_endpoint() {
    let registry = registry_on("cm-mute");
    let mut chain = chain_named("cm-mute-chain");
    chain
        .mix
        .endpoint_mut(MixerDirection::Output, "io", "Main")
        .muted = true;
    let rt = runtime(&chain, &registry);
    settle(&rt);
    assert_all(&callback(&rt), 0.0, 0.0, "muted chain output");
}

#[test]
fn chain_fader_multiplies_with_the_global_fader() {
    let registry = registry_on("cm-mul");
    set_endpoint_gain(MixerDirection::Input, "cm-mul", &[0], db(-6.0));
    let mut chain = chain_named("cm-mul-chain");
    chain
        .mix
        .endpoint_mut(MixerDirection::Input, "io", "Guitar")
        .gain_db = -6.0;
    let rt = runtime(&chain, &registry);
    settle(&rt);
    assert_all(
        &callback(&rt),
        LEVEL * db(-6.0) * db(-6.0),
        1e-5,
        "global -6 dB times chain -6 dB",
    );
}

#[test]
fn a_live_chain_fader_move_touches_only_its_own_chain() {
    // Two chains on the SAME physical endpoints: the chain fader is per
    // chain, so moving A's must leave B bit-identical.
    let registry = registry_on("cm-iso");
    let mut a = chain_named("cm-iso-a");
    let b = chain_named("cm-iso-b");
    let rt_a = runtime(&a, &registry);
    let rt_b = runtime(&b, &registry);
    settle(&rt_a);
    settle(&rt_b);
    a.mix
        .endpoint_mut(MixerDirection::Output, "io", "Main")
        .gain_db = -12.0;
    apply_chain_mix(&a, &registry);
    for _ in 0..2 {
        callback(&rt_a);
        callback(&rt_b);
    }
    assert_all(&callback(&rt_a), LEVEL * db(-12.0), 1e-5, "chain A");
    assert!(
        callback(&rt_b).iter().all(|&s| s == LEVEL),
        "chain B must not move"
    );
}

#[test]
fn chain_fader_back_to_unity_is_bit_identical_again() {
    let registry = registry_on("cm-back");
    let mut chain = chain_named("cm-back-chain");
    chain
        .mix
        .endpoint_mut(MixerDirection::Input, "io", "Guitar")
        .gain_db = -6.0;
    let rt = runtime(&chain, &registry);
    settle(&rt);
    chain.mix = Default::default();
    apply_chain_mix(&chain, &registry);
    callback(&rt);
    callback(&rt);
    assert!(
        callback(&rt).iter().all(|&s| s == LEVEL),
        "unity again must be exact"
    );
}

#[test]
fn di_fader_scales_the_di_loop() {
    let registry = registry_on("cm-di");
    let mut chain = chain_named("cm-di-chain");
    chain.mix.di_gain_db = -6.0;
    let rt = runtime(&chain, &registry);
    rt.set_di_loop(Some(Arc::new(DiLoop::from_samples(
        &[LEVEL; 4096],
        SR as u32,
        1,
        SR as u32,
        0,
    ))));
    settle(&rt);
    assert_all(&callback(&rt), LEVEL * db(-6.0), 1e-5, "DI -6 dB");
}

#[test]
fn an_untouched_di_fader_is_bit_identical() {
    let registry = registry_on("cm-di-unity");
    let chain = chain_named("cm-di-unity-chain");
    let rt = runtime(&chain, &registry);
    rt.set_di_loop(Some(Arc::new(DiLoop::from_samples(
        &[LEVEL; 4096],
        SR as u32,
        1,
        SR as u32,
        0,
    ))));
    settle(&rt);
    assert!(
        callback(&rt).iter().all(|&s| s == LEVEL),
        "unity DI must not touch a bit"
    );
}

#[test]
fn a_fader_survives_a_second_binding_on_the_same_physical_endpoint() {
    // Two bindings of one chain both play through the same stereo main: the
    // strip is one physical endpoint, so the fader moved on the first
    // binding's port must not be reset to unity by the second.
    let device = "cm-shared-out";
    let mut registry = registry_on(device);
    let mut second = registry[0].clone();
    second.id = "io-2".into();
    second.inputs[0].channels = vec![1];
    registry.push(second);
    let mut chain = chain_named("cm-shared-out-chain");
    chain.io_binding_ids.push("io-2".into());
    chain
        .mix
        .endpoint_mut(MixerDirection::Output, "io", "Main")
        .gain_db = -6.0;
    apply_chain_mix(&chain, &registry);
    assert_eq!(
        crate::chain_mix_gains::chain_endpoint_gain_target(
            &chain.id,
            MixerDirection::Output,
            device,
            &[0, 1]
        ),
        db(-6.0)
    );
}
