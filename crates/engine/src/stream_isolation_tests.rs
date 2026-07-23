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
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::AudioBlock;
use project::chain::Chain;
use project::project::Project;
use std::collections::HashMap;
use std::sync::Arc;

/// Build a registry input endpoint mirroring an old single-entry `InputBlock`.
/// `device`/`channels` carry through unchanged; mode follows the old
/// `ChainInputMode` → `ChannelMode` mapping.
pub(super) fn in_ep(name: &str, device: &str, mode: ChannelMode, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode,
        channels,
    }
}

/// Build a registry output endpoint mirroring an old single-entry `OutputBlock`.
pub(super) fn out_ep(name: &str, device: &str, mode: ChannelMode, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode,
        channels,
    }
}

/// One binding (`id: "io"`) holding the given input + output endpoints. The
/// chain selects it via `io_binding_ids: ["io"]`; head inputs and tail outputs
/// resolve from here — byte-identical device/mode/channels to the old entries.
pub(super) fn binding(inputs: Vec<IoEndpoint>, outputs: Vec<IoEndpoint>) -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs,
        outputs,
    }]
}

/// A model-A chain that selects the `"io"` binding and carries only effect
/// blocks (none for these passthrough isolation fixtures).
pub(super) fn bound_chain(id: &str, description: Option<String>, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

/// Registry for the "two guitars, one chain" scenario: two mono inputs on
/// DIFFERENT physical devices (guitar_a / guitar_b), one mono output.
fn dual_input_registry() -> Vec<IoBinding> {
    binding(
        vec![
            in_ep("in0", "guitar_a", ChannelMode::Mono, vec![0]),
            in_ep("in1", "guitar_b", ChannelMode::Mono, vec![0]),
        ],
        vec![out_ep("out0", "main_out", ChannelMode::Mono, vec![0])],
    )
}

/// Chain with N InputBlocks all routed to one OutputBlock. The user-
/// visible "two guitars in the same chain" scenario that triggered #350.
fn dual_input_chain() -> Chain {
    bound_chain("dual_input", Some("two guitars, one chain".into()), vec![])
}

fn build_dual_input_graph() -> RuntimeGraph {
    let chain = dual_input_chain();
    let registry = dual_input_registry();
    let project = Project {
        name: Some("stream_isolation_test".into()),
        chains: vec![chain.clone()],
        device_settings: Vec::new(),
        midi: None,
    };
    let mut sample_rates = HashMap::new();
    sample_rates.insert(chain.id.clone(), 48000.0_f32);
    let elastic_targets: HashMap<ChainId, Vec<usize>> = HashMap::new();
    build_runtime_graph(&project, &sample_rates, &elastic_targets, &registry)
        .expect("dual_input chain must build")
}

// ─────────────────────────────────────────────────────────────────────
// Contract: one runtime per InputBlock
// ─────────────────────────────────────────────────────────────────────

#[test]
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

// ─────────────────────────────────────────────────────────────────────────
// Regression tests (issue #350) — every effective input MUST receive data.
// Pinned here so the "channel 2 silent" class of bug never lands again.
// ─────────────────────────────────────────────────────────────────────────

/// 1 InputBlock with `mode: mono, channels: [0, 1]` is split by the engine
/// into 2 effective inputs (one per channel). For every effective input the
/// runtime MUST register at least one segment that fires when its CPAL
/// callback is dispatched. If any segment is missing, that channel is
/// silent — exactly the regression the previous broken iteration shipped.
#[test]
fn every_effective_input_index_has_at_least_one_segment() {
    // 1 InputBlock, 2 channels, mono mode — the user's "duas guitarras na
    // mesma chain" config.
    let registry = binding(
        vec![in_ep("in0", "scarlett", ChannelMode::Mono, vec![0, 1])],
        vec![out_ep("out0", "main_out", ChannelMode::Mono, vec![0])],
    );
    let chain = bound_chain("regression:every-input-has-segment", None, vec![]);

    let project = Project {
        name: Some("regression".into()),
        chains: vec![chain.clone()],
        device_settings: Vec::new(),
        midi: None,
    };
    let mut sample_rates = HashMap::new();
    sample_rates.insert(chain.id.clone(), 48_000.0_f32);
    let elastic_targets: HashMap<ChainId, Vec<usize>> = HashMap::new();
    let graph = build_runtime_graph(&project, &sample_rates, &elastic_targets, &registry)
        .expect("regression chain must build");

    let runtime = graph.chains.values().next().expect("expected 1 runtime");
    let processing = runtime.processing.lock().expect("lock poisoned");

    // Engine's effective_inputs splits mono multi-channel into one entry
    // per channel — so the user's chain produces ≥2 effective inputs.
    let (resolved_inputs, _) = crate::runtime_endpoints::resolve_chain_io(&chain, &registry);
    let (eff_inputs, cpal_indices, _, _) = effective_inputs(&chain, &resolved_inputs, &registry);
    assert!(
        eff_inputs.len() >= 2,
        "fixture invariant: 1-InputBlock mono multi-channel must split into ≥2 effective inputs, got {}",
        eff_inputs.len()
    );

    // Invariante: cada `cpal_index` que o engine declara como destino de
    // callback DEVE ter pelo menos um segment registrado em
    // `input_to_segments`. Quem declara uma stream cpal mas não registra
    // segment garante silêncio naquele callback — exatamente o que a
    // tentativa de fix anterior produziu pro canal 2.
    let unique_cpal_indices: std::collections::HashSet<usize> =
        cpal_indices.iter().copied().collect();
    for cpal_idx in unique_cpal_indices {
        let segments_for_idx = processing
            .input_to_segments
            .get(cpal_idx)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        assert!(
            !segments_for_idx.is_empty(),
            "input_to_segments[{}] is empty — engine declared cpal_index {} for a \
             callback but no segment is registered to receive its data; that \
             channel will be silent",
            cpal_idx,
            cpal_idx
        );
    }
}

/// Belt-and-suspenders: for every InputProcessingState the engine creates,
/// at least one entry in `input_to_segments` references it. If a segment
/// is orphaned (no callback dispatches to it), it never processes audio.
#[test]
fn no_segment_is_orphaned_from_input_dispatch() {
    let registry = binding(
        vec![in_ep("in0", "scarlett", ChannelMode::Mono, vec![0, 1, 2])],
        vec![out_ep("out0", "main_out", ChannelMode::Mono, vec![0])],
    );
    let chain = bound_chain("regression:no-orphan-segment", None, vec![]);
    let project = Project {
        name: Some("regression".into()),
        chains: vec![chain.clone()],
        device_settings: Vec::new(),
        midi: None,
    };
    let mut sample_rates = HashMap::new();
    sample_rates.insert(chain.id.clone(), 48_000.0_f32);
    let elastic_targets: HashMap<ChainId, Vec<usize>> = HashMap::new();
    let graph = build_runtime_graph(&project, &sample_rates, &elastic_targets, &registry)
        .expect("regression chain must build");

    let runtime = graph.chains.values().next().expect("expected 1 runtime");
    let processing = runtime.processing.lock().expect("lock poisoned");
    let input_states_len = processing.input_states.len();
    let input_to_segments_count = processing.input_to_segments.len();

    // Walk every segment index and assert SOME entry in input_to_segments
    // references it.
    for seg_idx in 0..input_states_len {
        let mut found = false;
        for input_idx in 0..input_to_segments_count {
            if processing.input_to_segments[input_idx].contains(&seg_idx) {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "segment {} is orphaned — no input_to_segments entry references it; \
             its audio is never processed",
            seg_idx
        );
    }
}

/// The output side mirror of the input regression: every output route the
/// engine creates MUST have at least one segment writing to it (i.e. some
/// `output_buffers` Arc points at it). An output route with no producer
/// is silent forever.
#[test]
fn every_output_route_has_at_least_one_producer_segment() {
    let registry = binding(
        vec![in_ep("in0", "scarlett", ChannelMode::Mono, vec![0])],
        vec![out_ep("out0", "main_out", ChannelMode::Mono, vec![0])],
    );
    let chain = bound_chain("regression:every-output-has-producer", None, vec![]);
    let project = Project {
        name: Some("regression".into()),
        chains: vec![chain.clone()],
        device_settings: Vec::new(),
        midi: None,
    };
    let mut sample_rates = HashMap::new();
    sample_rates.insert(chain.id.clone(), 48_000.0_f32);
    let elastic_targets: HashMap<ChainId, Vec<usize>> = HashMap::new();
    let graph = build_runtime_graph(&project, &sample_rates, &elastic_targets, &registry)
        .expect("regression chain must build");

    let runtime = graph.chains.values().next().expect("expected 1 runtime");
    let routes = runtime.output_routes.load_full();
    let processing = runtime.processing.lock().expect("lock poisoned");

    for (route_idx, _route) in routes.iter().enumerate() {
        // A producer is any InputProcessingState whose `output_route_indices`
        // names this route_idx — that segment will push frames to the route.
        let producers: Vec<usize> = processing
            .input_states
            .iter()
            .enumerate()
            .filter(|(_, state)| state.output_route_indices.contains(&route_idx))
            .map(|(i, _)| i)
            .collect();
        assert!(
            !producers.is_empty(),
            "output_routes[{}] has no producer segment — output is silent forever",
            route_idx
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Cross-talk / interference detection (issue #350) — measurable signal
// isolation between concurrent streams of the same device. THESE are the
// tests the user explicitly demanded ("garantir que streams sao isolados").
// ─────────────────────────────────────────────────────────────────────────

