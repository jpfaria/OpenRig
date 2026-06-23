//! Per-INPUT-port isolation within ONE binding (issue #716, Task 9 fix).
//!
//! The per-input router creates ONE fully isolated `ChainRuntimeState` per
//! input port: two inputs → two runtimes, summed at the backend POST per-input
//! limiter (CLAUDE.md invariant #4). A user can place N input + M output
//! endpoints on ONE binding (the all-to-all shape), e.g. two channels of the
//! same interface.
//!
//! `build_io_runtime_graph` must therefore still produce N isolated runtimes
//! for that single binding — one per INPUT port — NOT one shared runtime that
//! sums the inputs in a shared route accumulator PRE-limiter. A shared runtime
//! both violates invariant #4 (shared `mixed_per_route` / output route across
//! two input streams) and changes the sound (pre- vs post-limiter summing)
//! relative to the legacy backend sum.
//!
//! These tests pin:
//!   1. `count` — a one-binding 2-in/2-out chain builds exactly 2 runtimes.
//!   2. `isolation` — feeding input port 0 only leaves input port 1's runtime
//!      silent (no shared accumulator bleeding port 0's signal into port 1).
//!
//! Before the per-input split lands, the bound path packs both inputs into a
//! single `ChainRuntimeState`, so the count is 1 and feeding one input drives
//! the other input's segment in the shared runtime → both tests are RED.

use super::{build_io_runtime_graph, process_input_f32, process_output_f32};
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, InputBlock, OutputBlock};
use project::chain::Chain;
use project::project::Project;
use std::collections::HashMap;
use std::sync::Arc;

use super::ChainRuntimeState;

fn bound_input(id: &str, io: &str, endpoint: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: io.into(),
            endpoint: endpoint.into(),
            entries: Vec::new(),
        }),
    }
}

fn bound_output(id: &str, io: &str, endpoint: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: io.into(),
            endpoint: endpoint.into(),
            entries: Vec::new(),
        }),
    }
}

/// ONE binding with two mono input endpoints (distinct channels on the same
/// device) and two mono output endpoints — the all-to-all 2-in/2-out shape a
/// user can configure for a single interface.
fn one_all_to_all_binding() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io_x".into(),
        name: "Device X".into(),
        inputs: vec![
            IoEndpoint {
                name: "in0".into(),
                device_id: DeviceId("dev_x".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            },
            IoEndpoint {
                name: "in1".into(),
                device_id: DeviceId("dev_x".into()),
                mode: ChannelMode::Mono,
                channels: vec![1],
            },
        ],
        outputs: vec![
            IoEndpoint {
                name: "out0".into(),
                device_id: DeviceId("dev_x".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            },
            IoEndpoint {
                name: "out1".into(),
                device_id: DeviceId("dev_x".into()),
                mode: ChannelMode::Mono,
                channels: vec![1],
            },
        ],
    }]
}

/// Chain referencing the single binding's two inputs and two outputs —
/// a passthrough (no effect blocks) so the math is trivial.
fn one_binding_multi_io_chain() -> Chain {
    Chain {
        id: ChainId("one_binding_multi".into()),
        description: Some("per-input isolation within one binding".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            bound_input("in:0", "io_x", "in0"),
            bound_input("in:1", "io_x", "in1"),
            bound_output("out:0", "io_x", "out0"),
            bound_output("out:1", "io_x", "out1"),
        ],
    }
}

fn build_graph() -> super::RuntimeGraph {
    let chain = one_binding_multi_io_chain();
    let project = Project {
        name: Some("io_binding_input_isolation".into()),
        chains: vec![chain.clone()],
        device_settings: Vec::new(),
        midi: None,
    };
    let mut sample_rates = HashMap::new();
    sample_rates.insert(chain.id.clone(), 48_000.0_f32);
    let elastic_targets: HashMap<ChainId, Vec<usize>> = HashMap::new();
    build_io_runtime_graph(
        &project,
        &sample_rates,
        &elastic_targets,
        &one_all_to_all_binding(),
    )
    .expect("one-binding multi-io chain must build")
}

/// Total absolute energy a runtime emits on `out_route` over a steady-state
/// run, when its input `in_cpal` is driven at `level`.
fn energy(runtime: &Arc<ChainRuntimeState>, in_cpal: usize, level: f32, out_route: usize) -> f32 {
    let frames = 64usize;
    // Two-channel interleaved device frame (the migration multi-input shape
    // reads distinct channels of one device); only `in_cpal`'s channel carries
    // signal, the other is silent.
    let data: Vec<f32> = (0..frames * 2)
        .map(|i| if i % 2 == in_cpal { level } else { 0.0 })
        .collect();
    for _ in 0..16 {
        process_input_f32(runtime, in_cpal, &data, 2);
    }
    let mut total = 0.0_f32;
    for _ in 0..16 {
        let mut out = vec![0.0_f32; frames];
        process_output_f32(runtime, out_route, &mut out, 1);
        total += out.iter().map(|s| s.abs()).sum::<f32>();
    }
    total
}

#[test]
fn one_binding_two_inputs_build_two_isolated_runtimes() {
    let graph = build_graph();
    let count = graph
        .chains
        .keys()
        .filter(|(cid, _)| cid.0 == "one_binding_multi")
        .count();
    assert_eq!(
        count, 2,
        "a single binding with 2 input ports must build 2 ISOLATED runtimes \
         (one per input port, summed at the backend post-limiter — invariant #4); \
         got {count} runtime(s). One runtime means both inputs share a \
         processing/route accumulator PRE-limiter, violating isolation and \
         changing the sound vs the legacy backend sum."
    );
}

#[test]
fn feeding_one_input_leaves_the_other_inputs_runtime_silent() {
    let graph = build_graph();
    let mut runtimes: Vec<Arc<ChainRuntimeState>> = graph
        .chains
        .iter()
        .filter(|((cid, _), _)| cid.0 == "one_binding_multi")
        .map(|((_, group), rt)| (*group, rt.clone()))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|(_, rt)| rt)
        .collect();
    // Sort by the cpal input each runtime owns so [0] reads ch0, [1] reads ch1.
    runtimes.sort_by_key(|rt| rt.owned_entry.map(|(_, cpal)| cpal).unwrap_or(usize::MAX));
    assert_eq!(
        runtimes.len(),
        2,
        "expected 2 per-input runtimes; got {}",
        runtimes.len()
    );

    let rt0 = &runtimes[0];
    let rt1 = &runtimes[1];

    // Drive ONLY input port 0 (cpal 0) on its own runtime. Read input port 1's
    // runtime (cpal 1) without ever feeding it.
    let energy_rt0 = energy(rt0, 0, 0.5, 0);
    let mut leaked = 0.0_f32;
    for _ in 0..16 {
        let mut out = vec![0.0_f32; 64];
        process_output_f32(rt1, 0, &mut out, 1);
        leaked += out.iter().map(|s| s.abs()).sum::<f32>();
    }

    assert!(
        energy_rt0 > 1e-2,
        "input port 0's own runtime is silent ({energy_rt0:.6}) — its signal \
         did not reach its output"
    );
    assert!(
        leaked < 1e-3,
        "input port 0's signal leaked into input port 1's runtime \
         (leaked={leaked:.6}); the two inputs share state — isolation violated"
    );
}
