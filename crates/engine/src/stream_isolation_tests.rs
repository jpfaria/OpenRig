//! Stream-isolation contract tests (issue #350).
//!
//! Codifies the CLAUDE.md non-regression invariant #4: each `InputBlock`
//! must produce a TOTALLY isolated parallel stream. Two InputBlocks —
//! whether grouped under the same YAML chain or split across chains —
//! must NEVER share runtime state. No shared buffer, lock, scratch,
//! cache line, route, tap, or any Arc'd mutable state. CPU or buffer
//! contention from one stream must NOT affect the other's callback.
//!
//! Today the engine groups N inputs of one YAML chain into a single
//! `ChainRuntimeState` with shared `output_routes` (SPSC violation),
//! shared `input_taps`, shared `output_taps`, shared `processing`
//! mutex. The tests below assert the post-fix contract; until #350
//! is implemented they are `#[ignore]` (and will FAIL when run with
//! `--ignored`). When the fix lands in this branch, the `#[ignore]`
//! markers are dropped and the tests must pass.

use super::*;
use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::project::Project;
use std::collections::HashMap;
use std::sync::Arc;

fn input_block(id: &str, device: &str, channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId(device.into()),
                mode: ChainInputMode::Mono,
                channels,
            }],
        }),
    }
}

fn output_block(id: &str, device: &str, channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            entries: vec![OutputEntry {
                device_id: DeviceId(device.into()),
                mode: ChainOutputMode::Mono,
                channels,
            }],
        }),
    }
}

/// Chain with N InputBlocks all routed to one OutputBlock. The user-
/// visible "two guitars in the same chain" scenario that triggered #350.
fn dual_input_chain() -> Chain {
    Chain {
        id: ChainId("dual_input".into()),
        description: Some("two guitars, one chain".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        blocks: vec![
            input_block("dual_input:input:0", "guitar_a", vec![0]),
            input_block("dual_input:input:1", "guitar_b", vec![0]),
            output_block("dual_input:output:0", "main_out", vec![0]),
        ],
    }
}

fn build_dual_input_graph() -> RuntimeGraph {
    let chain = dual_input_chain();
    let project = Project {
        name: Some("stream_isolation_test".into()),
        chains: vec![chain.clone()],
        device_settings: Vec::new(),
    };
    let mut sample_rates = HashMap::new();
    sample_rates.insert(chain.id.clone(), 48000.0_f32);
    let elastic_targets: HashMap<ChainId, Vec<usize>> = HashMap::new();
    build_runtime_graph(&project, &sample_rates, &elastic_targets)
        .expect("dual_input chain must build")
}

// ─────────────────────────────────────────────────────────────────────
// Contract: one runtime per InputBlock
// ─────────────────────────────────────────────────────────────────────

#[test]
#[ignore = "PENDING #350 — engine groups all InputBlocks of a chain into ONE ChainRuntimeState"]
fn two_input_blocks_in_same_chain_produce_two_independent_runtimes() {
    let graph = build_dual_input_graph();

    assert!(
        graph.chains.len() >= 2,
        "expected one ChainRuntimeState per InputBlock (≥2), got {}",
        graph.chains.len()
    );
}

// ─────────────────────────────────────────────────────────────────────
// Contract: zero shared Arc'd state between streams
// ─────────────────────────────────────────────────────────────────────

#[test]
#[ignore = "PENDING #350 — output_routes are currently shared across InputBlocks of the same chain"]
fn two_input_blocks_must_not_share_output_routes_arc() {
    let graph = build_dual_input_graph();
    let runtimes: Vec<&Arc<ChainRuntimeState>> = graph.chains.values().collect();
    assert!(
        runtimes.len() >= 2,
        "fixture failed: dual_input_chain produced <2 runtimes"
    );

    for i in 0..runtimes.len() {
        for j in (i + 1)..runtimes.len() {
            let r0 = runtimes[i].output_routes.load_full();
            let r1 = runtimes[j].output_routes.load_full();
            assert!(
                !Arc::ptr_eq(&r0, &r1),
                "runtimes #{i} and #{j} share output_routes Vec Arc — violates isolation invariant"
            );
            for (k, (route0, route1)) in r0.iter().zip(r1.iter()).enumerate() {
                assert!(
                    !Arc::ptr_eq(route0, route1),
                    "runtimes #{i}/#{j} share OutputRoutingState Arc at index {k}"
                );
            }
        }
    }
}

#[test]
#[ignore = "PENDING #350 — input_taps Vec is currently shared across InputBlocks of the same chain"]
fn two_input_blocks_must_not_share_input_taps_arc() {
    let graph = build_dual_input_graph();
    let runtimes: Vec<&Arc<ChainRuntimeState>> = graph.chains.values().collect();
    assert!(runtimes.len() >= 2, "fixture failed");

    for i in 0..runtimes.len() {
        for j in (i + 1)..runtimes.len() {
            let r0 = runtimes[i].input_taps.load_full();
            let r1 = runtimes[j].input_taps.load_full();
            assert!(
                !Arc::ptr_eq(&r0, &r1),
                "runtimes #{i} and #{j} share input_taps Vec Arc — violates isolation invariant"
            );
        }
    }
}

#[test]
#[ignore = "PENDING #350 — processing scratch is currently shared across InputBlocks of the same chain"]
fn two_input_blocks_must_not_share_processing_state() {
    let graph = build_dual_input_graph();
    let runtimes: Vec<&Arc<ChainRuntimeState>> = graph.chains.values().collect();
    assert!(runtimes.len() >= 2, "fixture failed");

    // The processing Mutex protects per-input scratch, segment maps, and
    // input_states. Sharing the Mutex itself between streams means a slow
    // input contends with another's callback. Address-equality of the
    // Mutex object is sufficient evidence of a shared lock.
    for i in 0..runtimes.len() {
        for j in (i + 1)..runtimes.len() {
            let p0: *const _ = &runtimes[i].processing;
            let p1: *const _ = &runtimes[j].processing;
            assert!(
                p0 != p1,
                "runtimes #{i} and #{j} reference the same processing Mutex — \
                 contention from one input's callback can stall the other"
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Contract: ElasticBuffer obeys SPSC — each output buffer has exactly
// one producer (one InputBlock).
// ─────────────────────────────────────────────────────────────────────

#[test]
#[ignore = "PENDING #350 — output route ElasticBuffer is currently a single instance shared by N InputBlocks pushing concurrently"]
fn each_output_route_buffer_has_exactly_one_producer() {
    // The ElasticBuffer field of OutputRoutingState is declared SPSC
    // (`crates/engine/src/runtime.rs:90-99`). With multiple InputBlocks
    // routing to the same OutputBlock, today the engine has each input's
    // process_input_f32 call `route.buffer.push()` on the SAME ElasticBuffer.
    // Two producers on an SPSC ring is undefined behavior + cache contention.
    //
    // Post-#350 the architecture is: each InputBlock owns its own
    // OutputRoutingState (and its own ElasticBuffer). If a user asks for
    // "two guitars merging into one device output", the merge happens at
    // the cpal/JACK backend level — not by stuffing two producers into
    // one of OUR rings.
    let graph = build_dual_input_graph();
    let runtimes: Vec<&Arc<ChainRuntimeState>> = graph.chains.values().collect();
    assert!(runtimes.len() >= 2, "fixture failed");

    // Each runtime must have its own distinct ElasticBuffer instance
    // (not just a different Arc<OutputRoutingState> wrapper around the
    // same buffer — different ElasticBuffers).
    let buffers: Vec<*const _> = runtimes
        .iter()
        .flat_map(|r| {
            let routes = r.output_routes.load_full();
            (0..routes.len())
                .map(|i| {
                    let r = routes[i].clone();
                    let p: *const _ = &r.buffer;
                    p
                })
                .collect::<Vec<_>>()
        })
        .collect();

    for i in 0..buffers.len() {
        for j in (i + 1)..buffers.len() {
            assert!(
                buffers[i] != buffers[j],
                "ElasticBuffer instance shared across output routes — \
                 two producers on a single SPSC ring violates SPSC contract"
            );
        }
    }
}
