//! Issue #969 — after a scene switch the chain plays late and garbled until it
//! is toggled off and on. Measured live on the owner's rig while it happened
//! (`openrig://routes`): every output route of the chain sat at
//! `fill_frames: 1024` — its ring completely FULL at a 64-frame device buffer
//! (~23 ms behind, and every push dropping a frame) — with `latency_trims: 0`.
//!
//! The #953 guard sheds latency a stall leaves in the ring, but it measures
//! "too much" against the lowest fill the route has shown. A route whose ring
//! filled up BEFORE its output stream began popping (the producer ran ahead
//! while the new streams came up) shows "full" from the very first window, so
//! full becomes its proven level and nothing is ever above it. A live rebuild
//! then reuses that route (#670), so only a manual off/on — brand-new routes —
//! brings the chain back.
//!
//! The route was built for a cushion (its target); a floor held above that
//! target is stuck latency no matter when it appeared.

use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use project::chain::Chain;

const SR: f32 = 48_000.0;
const FRAMES: usize = 64;
/// The owner's measured ring: target 512 → capacity 1024.
const TARGET: usize = 512;
/// Input callbacks that land before the output stream pops once — enough to
/// fill the ring.
const HEAD_START_CALLBACKS: usize = 24;
/// Lockstep callbacks afterwards: ~40 guard windows.
const LOCKSTEP_CALLBACKS: usize = 5_000;
/// Growth the guard tolerates above the level (`elastic_drift_guard::SLACK_FRAMES`).
const SLACK: usize = 32;

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
            mode: domain::io_binding::ChannelMode::Mono,
            channels: vec![0],
        }],
    }]
}

fn passthrough_chain() -> Chain {
    Chain {
        id: ChainId("chain:969:passthrough".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

fn runtime() -> Arc<engine::runtime::ChainRuntimeState> {
    Arc::new(
        engine::runtime::build_chain_runtime_state(
            &passthrough_chain(),
            SR,
            &[TARGET],
            &registry(),
        )
        .expect("passthrough runtime must build"),
    )
}

fn sine(cb: usize) -> Vec<f32> {
    (0..FRAMES)
        .map(|i| {
            let n = (cb * FRAMES + i) as f32;
            0.5 * (2.0 * std::f32::consts::PI * 220.0 * n / SR).sin()
        })
        .collect()
}

/// Input and output in lockstep, one period each, like a healthy duplex pair.
fn lockstep(runtime: &Arc<engine::runtime::ChainRuntimeState>, from_cb: usize) {
    let mut out = vec![0.0_f32; FRAMES];
    for cb in from_cb..from_cb + LOCKSTEP_CALLBACKS {
        engine::runtime::process_input_f32(runtime, 0, &sine(cb), 1);
        engine::runtime::process_output_f32(runtime, 0, &mut out, 1);
    }
}

fn route(
    runtime: &engine::runtime::ChainRuntimeState,
) -> engine::runtime_output_route_stats::OutputRouteStats {
    runtime
        .take_output_route_stats()
        .into_iter()
        .next()
        .expect("the chain owns one output route")
}

#[test]
fn a_ring_that_filled_before_the_output_started_is_shed_back_to_its_target() {
    let runtime = runtime();

    // The new streams come up and the input runs ahead of the output.
    for cb in 0..HEAD_START_CALLBACKS {
        engine::runtime::process_input_f32(&runtime, 0, &sine(cb), 1);
    }
    let stuck = route(&runtime).fill_frames;
    assert!(
        stuck > TARGET + SLACK,
        "setup: the head start must leave the ring above its target (fill {stuck})"
    );

    lockstep(&runtime, HEAD_START_CALLBACKS);

    let after = route(&runtime);
    eprintln!(
        "[#969] fill {} -> {} (target {TARGET}), trims {}, underruns {}",
        stuck, after.fill_frames, after.latency_trims, after.underruns
    );
    assert!(
        after.fill_frames <= TARGET + SLACK,
        "#969: the route stayed {} frames deep (target {TARGET}) after {LOCKSTEP_CALLBACKS} \
         lockstep callbacks with {} trims — the live symptom: fill_frames 1024, \
         latency_trims 0, chain late and garbled until toggled off/on",
        after.fill_frames,
        after.latency_trims
    );
}

#[test]
fn a_route_that_never_ran_ahead_is_never_trimmed() {
    let runtime = runtime();
    lockstep(&runtime, 0);
    let after = route(&runtime);
    assert_eq!(
        after.latency_trims, 0,
        "#969: a healthy lockstep route must never be trimmed (fill {})",
        after.fill_frames
    );
    assert!(after.fill_frames <= TARGET + SLACK);
}
