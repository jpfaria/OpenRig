//! Cross-binding isolation acceptance (issue #716, Task 8).
//!
//! The per-binding routing model (CLAUDE.md invariant #4) requires that a
//! stream is spawned ONLY for an `(input port, output port)` pair belonging
//! to the SAME io binding. Structurally, the input of binding A can therefore
//! never reach the output of binding B.
//!
//! This test builds a chain that references two distinct bindings, `io_a`
//! (input + output on device A) and `io_b` (input + output on device B).
//! It feeds a signal into A's input callback and silence into B's, processes
//! one block, and asserts A's output route carries the signal while B's stays
//! silent. Then it mirrors the test (feed B, assert A silent).
//!
//! Before the per-binding routing rule lands, the chain-shared cartesian
//! routing pairs EVERY effective input with EVERY output route, so A's signal
//! bleeds into B's output → this test is RED.

use super::{build_io_runtime_graph, process_input_f32, process_output_f32};
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, InputBlock, OutputBlock};
use project::chain::Chain;
use project::project::Project;
use std::collections::HashMap;

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

/// Registry with two single-device mono bindings.
fn two_bindings() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "io_a".into(),
            name: "Device A".into(),
            inputs: vec![IoEndpoint {
                name: "in_a".into(),
                device_id: DeviceId("dev_a".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
            outputs: vec![IoEndpoint {
                name: "out_a".into(),
                device_id: DeviceId("dev_a".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
        },
        IoBinding {
            id: "io_b".into(),
            name: "Device B".into(),
            inputs: vec![IoEndpoint {
                name: "in_b".into(),
                device_id: DeviceId("dev_b".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
            outputs: vec![IoEndpoint {
                name: "out_b".into(),
                device_id: DeviceId("dev_b".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
        },
    ]
}

/// Chain: input(io_a) → input(io_b) → output(io_a) → output(io_b).
/// A passthrough chain (no effect blocks) keeps the math trivial: the input
/// signal arrives unchanged at its binding's output.
fn two_binding_chain() -> Chain {
    Chain {
        id: ChainId("two_binding".into()),
        description: Some("cross-binding isolation".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            bound_input("in:a", "io_a", "in_a"),
            bound_input("in:b", "io_b", "in_b"),
            bound_output("out:a", "io_a", "out_a"),
            bound_output("out:b", "io_b", "out_b"),
        ],
    }
}

/// Returns `(cpal_index_for_a_input, route_index_for_a_output,
/// cpal_index_for_b_input, route_index_for_b_output)`.
///
/// The engine orders effective inputs by block order (io_a input first, io_b
/// second) and effective outputs likewise (io_a output first, io_b second).
fn ab_indices() -> (usize, usize, usize, usize) {
    (0, 0, 1, 1)
}

fn build_graph() -> super::RuntimeGraph {
    let chain = two_binding_chain();
    let project = Project {
        name: Some("io_binding_isolation".into()),
        chains: vec![chain.clone()],
        device_settings: Vec::new(),
        midi: None,
    };
    let mut sample_rates = HashMap::new();
    sample_rates.insert(chain.id.clone(), 48_000.0_f32);
    let elastic_targets: HashMap<ChainId, Vec<usize>> = HashMap::new();
    build_io_runtime_graph(&project, &sample_rates, &elastic_targets, &two_bindings())
        .expect("two-binding chain must build")
}

/// The per-input runtime that owns cpal input `cpal`. Each input PORT is its
/// own isolated runtime (issue #716 fix), so the two bindings' inputs land in
/// two distinct runtimes — cross-binding isolation is now structural at the
/// runtime level, not just route routing within one runtime.
fn runtime_for_cpal(
    graph: &super::RuntimeGraph,
    cpal: usize,
) -> std::sync::Arc<super::ChainRuntimeState> {
    graph
        .chains
        .values()
        .find(|rt| rt.input_cpal_index() == Some(cpal))
        .unwrap_or_else(|| panic!("no runtime owns cpal input {cpal}"))
        .clone()
}

/// Pump `frames` blocks of `level` through input `in_cpal`, then drain output
/// route `out_route`. Returns the summed absolute energy of the drained output.
fn energy_through(
    runtime: &std::sync::Arc<super::ChainRuntimeState>,
    in_cpal: usize,
    level: f32,
    out_route: usize,
) -> f32 {
    let frames = 64usize;
    let data: Vec<f32> = vec![level; frames];
    // Prime several callbacks so the elastic buffer fills past its cushion.
    for _ in 0..16 {
        process_input_f32(runtime, in_cpal, &data, 1);
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
fn signal_into_binding_a_does_not_reach_binding_b_output() {
    let graph = build_graph();
    let (in_a, out_a, _in_b, out_b) = ab_indices();
    // Each binding's input is its own per-input runtime.
    let rt_a = runtime_for_cpal(&graph, in_a);
    let rt_b = runtime_for_cpal(&graph, _in_b);

    // Feed a strong signal into A's input only (on A's runtime).
    let energy_a = energy_through(&rt_a, in_a, 0.5, out_a);
    // Drain B's output on B's runtime, which was never fed.
    let mut out = vec![0.0_f32; 64];
    let mut energy_b = 0.0_f32;
    for _ in 0..16 {
        process_output_f32(&rt_b, out_b, &mut out, 1);
        energy_b += out.iter().map(|s| s.abs()).sum::<f32>();
    }

    assert!(
        energy_a > 1e-2,
        "binding A output is silent ({energy_a:.6}) — A's own signal did not reach its output"
    );
    assert!(
        energy_b < 1e-3,
        "binding A's signal bled into binding B's output (energy_b={energy_b:.6}); \
         cross-binding isolation violated"
    );
}

#[test]
fn signal_into_binding_b_does_not_reach_binding_a_output() {
    let graph = build_graph();
    let (in_a, out_a, in_b, out_b) = ab_indices();
    let rt_a = runtime_for_cpal(&graph, in_a);
    let rt_b = runtime_for_cpal(&graph, in_b);

    let energy_b = energy_through(&rt_b, in_b, 0.5, out_b);
    let mut out = vec![0.0_f32; 64];
    let mut energy_a = 0.0_f32;
    for _ in 0..16 {
        process_output_f32(&rt_a, out_a, &mut out, 1);
        energy_a += out.iter().map(|s| s.abs()).sum::<f32>();
    }

    assert!(
        energy_b > 1e-2,
        "binding B output is silent ({energy_b:.6}) — B's own signal did not reach its output"
    );
    assert!(
        energy_a < 1e-3,
        "binding B's signal bled into binding A's output (energy_a={energy_a:.6}); \
         cross-binding isolation violated"
    );
}
