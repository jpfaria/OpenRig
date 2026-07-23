//! Same-device per-entry stream isolation (issue #703, follow-up of #350).
//!
//! #350 delivered per-DEVICE isolation: the runtime graph partitions a
//! chain's segments by cpal input index, so two InputBlocks on two
//! physical devices get two fully isolated runtimes. But two input
//! entries on the SAME physical device (e.g. Scarlett ch0 + ch1 — the
//! "two guitars, one interface" case) share one cpal index and therefore
//! one runtime: one `processing` Mutex, one `output_routes` Vec, one
//! `input_taps` Vec. That violates CLAUDE.md invariant #4 for the
//! same-device topology.
//!
//! The post-fix contract pinned here: the graph partitions by RAW input
//! entry, not by device. Split-mono siblings (ONE raw entry with
//! `mode: mono, channels: [a, b]`) are NOT entries — they stay together
//! in one runtime so the pinned volume invariants (g02/g03: split-mono
//! dual SUMS before the limiter) keep their exact math.

use super::*;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;
use project::project::Project;
use std::collections::HashMap;
use std::sync::Arc;

/// Registry id every chain in this file references via
/// `io_binding_ids: vec!["io".into()]`.
const IO_BINDING_ID: &str = "io";

/// Two RAW input entries on the SAME physical device (channel 0 and
/// channel 1 of one interface) — two separate endpoints, preserved in
/// order — both feeding one stereo output endpoint. The user-visible
/// "two guitars on one interface" scenario.
fn same_device_dual_entry_registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: IO_BINDING_ID.into(),
        name: "IO".into(),
        inputs: vec![
            IoEndpoint {
                name: "in0".into(),
                device_id: DeviceId("shared_iface".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            },
            IoEndpoint {
                name: "in1".into(),
                device_id: DeviceId("shared_iface".into()),
                mode: ChannelMode::Mono,
                channels: vec![1],
            },
        ],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("main_out".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0],
        }],
    }]
}

/// ONE raw entry split by the engine into two mono streams. These are
/// split-mono SIBLINGS, not independent entries — they must stay in the
/// same runtime (pinned volume invariants g02/g03 sum them before the
/// per-runtime limiter).
fn split_mono_registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: IO_BINDING_ID.into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("shared_iface".into()),
            mode: ChannelMode::Mono,
            channels: vec![0, 1],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("main_out".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0],
        }],
    }]
}

fn same_device_dual_entry_chain() -> Chain {
    Chain {
        id: ChainId("same_dev".into()),
        description: Some("two guitars, one interface".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![IO_BINDING_ID.into()],
        blocks: vec![],
        di_output: None,
    }
}

fn split_mono_chain() -> Chain {
    Chain {
        id: ChainId("split_mono".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![IO_BINDING_ID.into()],
        blocks: vec![],
        di_output: None,
    }
}

fn build_graph(chain: &Chain, registry: &[IoBinding]) -> RuntimeGraph {
    let project = Project {
        name: Some("same_device_isolation_test".into()),
        chains: vec![chain.clone()],
        device_settings: Vec::new(),
        midi: None,
    };
    let mut sample_rates = HashMap::new();
    sample_rates.insert(chain.id.clone(), 48_000.0_f32);
    let elastic_targets: HashMap<ChainId, Vec<usize>> = HashMap::new();
    build_runtime_graph(&project, &sample_rates, &elastic_targets, registry)
        .expect("same-device chain must build")
}

// ─────────────────────────────────────────────────────────────────────
// Contract: one runtime per RAW input entry, even on a shared device
// ─────────────────────────────────────────────────────────────────────

#[test]
fn two_entries_on_same_device_produce_two_independent_runtimes() {
    let chain = same_device_dual_entry_chain();
    let graph = build_graph(&chain, &same_device_dual_entry_registry());

    let runtimes = graph.runtimes_with_groups_for(&chain.id);
    assert_eq!(
        runtimes.len(),
        2,
        "two raw input entries (even on ONE device) => two isolated \
         per-entry runtimes, got {}",
        runtimes.len()
    );
    assert!(
        !Arc::ptr_eq(&runtimes[0].1, &runtimes[1].1),
        "each input entry must own a SEPARATE ChainRuntimeState"
    );
}

#[test]
fn same_device_runtimes_must_not_share_output_routes_arc() {
    let chain = same_device_dual_entry_chain();
    let graph = build_graph(&chain, &same_device_dual_entry_registry());
    let runtimes: Vec<&Arc<ChainRuntimeState>> = graph.chains.values().collect();
    assert!(
        runtimes.len() >= 2,
        "fixture failed: same-device dual-entry chain produced <2 runtimes"
    );

    for i in 0..runtimes.len() {
        for j in (i + 1)..runtimes.len() {
            let r0 = runtimes[i].output_routes.load_full();
            let r1 = runtimes[j].output_routes.load_full();
            assert!(
                !Arc::ptr_eq(&r0, &r1),
                "same-device runtimes #{i} and #{j} share output_routes Vec Arc"
            );
            for (k, (route0, route1)) in r0.iter().zip(r1.iter()).enumerate() {
                assert!(
                    !Arc::ptr_eq(route0, route1),
                    "same-device runtimes #{i}/#{j} share OutputRoutingState Arc at index {k}"
                );
            }
        }
    }
}

#[test]
fn same_device_runtimes_must_not_share_processing_state() {
    let chain = same_device_dual_entry_chain();
    let graph = build_graph(&chain, &same_device_dual_entry_registry());
    let runtimes: Vec<&Arc<ChainRuntimeState>> = graph.chains.values().collect();
    assert!(
        runtimes.len() >= 2,
        "fixture failed: same-device dual-entry chain produced <2 runtimes"
    );

    for i in 0..runtimes.len() {
        for j in (i + 1)..runtimes.len() {
            let p0: *const _ = &runtimes[i].processing;
            let p1: *const _ = &runtimes[j].processing;
            assert!(
                p0 != p1,
                "same-device runtimes #{i} and #{j} reference the same processing \
                 Mutex — contention from one entry's callback can stall the other"
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Contract: split-mono siblings are ONE entry — they stay together
// ─────────────────────────────────────────────────────────────────────

#[test]
fn split_mono_siblings_stay_in_one_runtime() {
    let chain = split_mono_chain();
    let registry = split_mono_registry();
    let graph = build_graph(&chain, &registry);

    assert_eq!(
        graph.chains.len(),
        1,
        "split-mono siblings come from ONE raw entry and must stay in one \
         runtime — separating them would double-limit the pinned g02/g03 sum"
    );
}

// ─────────────────────────────────────────────────────────────────────
// Contract: an in-place chain edit keeps each runtime entry-local
// ─────────────────────────────────────────────────────────────────────

/// A volume/knob edit takes the in-place `upsert_chain` path (topology
/// unchanged). Each per-entry runtime must be refilled with ONLY its own
/// entry's segments: both entries share cpal index 0, so if the update
/// refills every runtime with ALL segments, the one device callback
/// dispatches BOTH segments in BOTH runtimes — the same guitar processed
/// twice and summed (audible double volume).
#[test]
fn in_place_upsert_keeps_same_device_runtimes_entry_local() {
    let chain = same_device_dual_entry_chain();
    let registry = same_device_dual_entry_registry();
    let mut graph = build_graph(&chain, &registry);

    let mut edited = chain.clone();
    edited.volume = 80.0;
    graph
        .upsert_chain(&edited, 48_000.0, &HashMap::new(), false, &[], &registry)
        .expect("in-place upsert must succeed");

    let runtimes = graph.runtimes_with_groups_for(&chain.id);
    assert_eq!(runtimes.len(), 2, "topology unchanged => still 2 runtimes");
    for (group, runtime) in &runtimes {
        let processing = runtime.processing.lock().expect("processing lock");
        assert_eq!(
            processing.input_states.len(),
            1,
            "runtime of entry group {group} must hold ONLY its own entry's \
             segment after an in-place edit, got {} input states",
            processing.input_states.len()
        );
        let dispatched: usize = processing
            .input_to_segments
            .iter()
            .map(|segs| segs.len())
            .sum();
        assert_eq!(
            dispatched, 1,
            "runtime of entry group {group} must dispatch exactly its own \
             segment, got {dispatched} — duplicated dispatch doubles the audio"
        );
    }
}
