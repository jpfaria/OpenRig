//! Issue #716 — the per-binding routing engine runs on the LIVE build path.
//!
//! The production seam is `BuildRequest` → `build_chain_runtime` (the worker
//! payload the controller submits). A BOUND chain is routed PER BINDING against
//! `io_bindings`: one isolated runtime per input port, cross-binding isolation
//! structural. Clean break (#716): routing is binding-only — an UNBOUND chain
//! (empty `io`, even if it still carries legacy `entries`) builds NO runtime;
//! there is no fallback to the legacy `entries` all-to-all path.
//!
//! These tests drive the LIVE seam with a registry and assert both behaviours.

use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{process_input_f32, process_output_f32, ChainRuntimeState};
use infra_cpal::{build_chain_runtime, BuildRequest};
use project::block::{AudioBlock, AudioBlockKind, InputBlock, OutputBlock};
use project::chain::Chain;

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

/// Two single-device mono bindings (A on dev_a, B on dev_b).
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

/// Chain referencing both bindings, no effect blocks (passthrough).
fn two_binding_chain() -> Chain {
    Chain {
        id: ChainId("two_binding".into()),
        description: Some("live-path cross-binding isolation".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![
            bound_input("in:a", "io_a", "in_a"),
            bound_input("in:b", "io_b", "in_b"),
            bound_output("out:a", "io_a", "out_a"),
            bound_output("out:b", "io_b", "out_b"),
        ],
    }
}

fn runtime_for_cpal(
    runtimes: &[(usize, Arc<ChainRuntimeState>)],
    cpal: usize,
) -> Arc<ChainRuntimeState> {
    runtimes
        .iter()
        .find(|(_, rt)| rt.input_cpal_index() == Some(cpal))
        .map(|(_, rt)| rt.clone())
        .unwrap_or_else(|| panic!("no live-path runtime owns cpal input {cpal}"))
}

/// Pump `level` into input `in_cpal`, drain output route `out_route`, return
/// summed absolute energy.
fn energy_through(
    runtime: &Arc<ChainRuntimeState>,
    in_cpal: usize,
    level: f32,
    out_route: usize,
) -> f32 {
    let frames = 64usize;
    let data: Vec<f32> = vec![level; frames];
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
fn live_path_builds_one_isolated_runtime_per_input_binding() {
    let req = BuildRequest {
        chain: two_binding_chain(),
        sample_rate: 48_000.0,
        buffer_sizes: vec![1024, 1024],
        io_bindings: two_bindings(),
    };
    let runtimes = build_chain_runtime(&req).expect("bound chain must build on the live path");
    // Two distinct input bindings ⇒ two isolated per-input runtimes. The legacy
    // dead-code path produced a single fallback runtime for a drained-entries
    // chain.
    assert_eq!(
        runtimes.len(),
        2,
        "bound chain must produce one isolated runtime per input binding"
    );
    // Each binding's input lands on its own cpal index.
    assert!(runtimes
        .iter()
        .any(|(_, rt)| rt.input_cpal_index() == Some(0)));
    assert!(runtimes
        .iter()
        .any(|(_, rt)| rt.input_cpal_index() == Some(1)));
}

/// Clean break (#716): a legacy/unbound chain — Input/Output blocks with
/// `entries` but empty `io` — must build NO runtime on the production seam.
/// Routing is binding-only; the user reconfigures via the registry. The
/// production fallback to the legacy `entries` all-to-all path is gone.
#[test]
fn live_path_unbound_chain_builds_no_runtime() {
    use project::block::{InputEntry, OutputEntry};
    use project::chain::{ChainInputMode, ChainOutputMode};

    let legacy_chain = Chain {
        id: ChainId("legacy".into()),
        description: Some("legacy entries-only chain".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![
            AudioBlock {
                id: BlockId("in:legacy".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    io: String::new(),
                    endpoint: String::new(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev_a".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
            AudioBlock {
                id: BlockId("out:legacy".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    io: String::new(),
                    endpoint: String::new(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("dev_a".into()),
                        mode: ChainOutputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
        ],
    };
    let req = BuildRequest {
        chain: legacy_chain,
        sample_rate: 48_000.0,
        buffer_sizes: vec![1024],
        io_bindings: Vec::new(),
    };
    let runtimes = build_chain_runtime(&req)
        .expect("unbound chain must build cleanly (no runtimes, no error)");
    assert!(
        runtimes.is_empty(),
        "unbound (legacy entries-only) chain must produce NO runtime — routing is binding-only, \
         got {} runtime(s)",
        runtimes.len()
    );
}

#[test]
fn live_path_cross_binding_isolation_holds() {
    let req = BuildRequest {
        chain: two_binding_chain(),
        sample_rate: 48_000.0,
        buffer_sizes: vec![1024, 1024],
        io_bindings: two_bindings(),
    };
    let runtimes = build_chain_runtime(&req).expect("bound chain must build on the live path");

    let rt_a = runtime_for_cpal(&runtimes, 0);
    let rt_b = runtime_for_cpal(&runtimes, 1);

    // Feed binding A's input; A's output route is index 0, B's is index 1.
    let energy_a = energy_through(&rt_a, 0, 0.5, 0);
    let mut out = vec![0.0_f32; 64];
    let mut energy_b = 0.0_f32;
    for _ in 0..16 {
        process_output_f32(&rt_b, 1, &mut out, 1);
        energy_b += out.iter().map(|s| s.abs()).sum::<f32>();
    }

    assert!(
        energy_a > 1e-2,
        "binding A output silent ({energy_a:.6}) — its own signal did not reach its output on the live path"
    );
    assert!(
        energy_b < 1e-3,
        "binding A bled into binding B output (energy_b={energy_b:.6}) — live-path isolation violated"
    );
}
