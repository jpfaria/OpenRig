//! Engine runtime tests (issue #792 split from runtime_tests.rs).
//! Grouped by responsibility; shared fixtures live in `runtime_tests.rs`.
#![allow(unused_imports)]
use super::*;
use super::tests::*;


// ── build_runtime_graph edge cases ───────────────────────────────────────

#[test]
fn build_runtime_graph_skips_disabled_chains() {
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("disabled".into()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![],
            di_output: None,
        }],
        midi: None,
    };

    let runtime = build_runtime_graph(&project, &HashMap::new(), &HashMap::new(), &[])
        .expect("build should succeed with disabled chain");
    assert!(
        runtime.chains.is_empty(),
        "disabled chains should be skipped"
    );
}


// ── RuntimeGraph methods ─────────────────────────────────────────────────

#[test]
fn runtime_graph_remove_chain_removes_entry() {
    let chain = tuner_track("chain:remove", Vec::new());
    let mut rates = HashMap::new();
    rates.insert(ChainId("chain:remove".into()), 48_000.0);
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain],
        midi: None,
    };
    let mut graph = build_runtime_graph(&project, &rates, &HashMap::new(), &[]).unwrap();
    assert_eq!(graph.chains.len(), 1);
    graph.remove_chain(&ChainId("chain:remove".into()));
    assert!(graph.chains.is_empty());
}


#[test]
fn runtime_graph_runtime_for_chain_returns_none_for_unknown() {
    use super::RuntimeGraph;
    let graph = RuntimeGraph {
        chains: HashMap::new(),
    };
    assert!(graph
        .runtime_for_chain(&ChainId("nonexistent".into()))
        .is_none());
}


#[test]
fn runtime_graph_upsert_chain_creates_new_entry() {
    use super::RuntimeGraph;
    let mut graph = RuntimeGraph {
        chains: HashMap::new(),
    };
    let chain = tuner_track("chain:new", Vec::new());
    let result = graph.upsert_chain(
        &chain,
        48_000.0,
        &HashMap::new(),
        false,
        &[DEFAULT_ELASTIC_TARGET],
        &[],
    );
    assert!(result.is_ok());
    assert_eq!(graph.chains.len(), 1);
}


#[test]
fn runtime_graph_upsert_chain_updates_existing() {
    use super::RuntimeGraph;
    let mut graph = RuntimeGraph {
        chains: HashMap::new(),
    };
    let chain = tuner_track("chain:upsert", vec![tuner_block("b:0", 440.0)]);
    graph
        .upsert_chain(
            &chain,
            48_000.0,
            &HashMap::new(),
            false,
            &[DEFAULT_ELASTIC_TARGET],
            &[],
        )
        .unwrap();
    // Update — should reuse existing entry
    let chain2 = tuner_track("chain:upsert", vec![tuner_block("b:0", 445.0)]);
    let result = graph.upsert_chain(
        &chain2,
        48_000.0,
        &HashMap::new(),
        false,
        &[DEFAULT_ELASTIC_TARGET],
        &[],
    );
    assert!(result.is_ok());
    assert_eq!(graph.chains.len(), 1);
}


#[test]
fn runtime_graph_upsert_chain_propagates_volume_change_to_live_runtime() {
    // Reproduces the exact path the volume slider takes:
    //   slider → ChainCommand::SetChainVolume (mutates Project.chain.volume)
    //   → sync_live_chain_runtime → controller.upsert_chain_with_resolved
    //   → RuntimeGraph::upsert_chain (called unconditionally, ln 501 controller.rs)
    // The audio thread reads `runtime.volume_pct()` every output callback,
    // so after a volume edit on a LIVE (already-running) chain the runtime's
    // volume_pct MUST reflect the new value — without this, moving the
    // slider does nothing audible.
    use super::RuntimeGraph;
    let mut graph = RuntimeGraph {
        chains: HashMap::new(),
    };
    let mut chain = io_passthrough_chain("chain:vol");
    chain.volume = 100.0;
    graph
        .upsert_chain(
            &chain,
            48_000.0,
            &HashMap::new(),
            false,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_mono(),
        )
        .unwrap();
    let rt = graph.runtime_for_chain(&chain.id).expect("runtime exists");
    assert_eq!(rt.volume_pct(), 100.0, "initial volume must be 100");

    // User drags the slider → 150. Same call the controller makes for a
    // live chain (needs_stream_rebuild = false → fast in-place path).
    chain.volume = 150.0;
    graph
        .upsert_chain(
            &chain,
            48_000.0,
            &HashMap::new(),
            false,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_mono(),
        )
        .unwrap();
    let rt2 = graph.runtime_for_chain(&chain.id).expect("runtime exists");
    assert_eq!(
        rt2.volume_pct(),
        150.0,
        "volume change did NOT reach the live runtime — slider is dead"
    );
}


#[test]
fn runtime_graph_upsert_volume_change_reaches_runtime_held_by_callback_multi_input() {
    // The user's real bug: a chain with TWO input devices (CPM 22:
    // Scarlett + TEYUN). The cpal callbacks capture `Arc<ChainRuntimeState>`
    // at stream-build time. A volume edit does NOT change the stream
    // signature, so `needs_stream_rebuild = false` and the callbacks keep
    // their original Arcs. If `upsert_chain`'s multi-input path drops those
    // Arcs and inserts brand-new ones, the live callbacks read the OLD
    // volume forever — moving the slider does nothing for multi-input chains.
    //
    // This test simulates the callback by holding the Arcs obtained BEFORE
    // the volume edit, then asserts they observe the new volume after the
    // same `upsert_chain(needs_stream_rebuild=false)` the controller runs.
    use super::RuntimeGraph;

    fn two_device_chain(id: &str, volume: f32) -> Chain {
        // Two input endpoints (scarlett + teyun) resolve from
        // `io_registry_two_device()`; head/tail no longer live in the chain.
        Chain {
            id: ChainId(id.into()),
            description: Some("two guitars".into()),
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume,
            io_binding_ids: vec![IO_BINDING_ID.into()],
            blocks: vec![],
            di_output: None,
        }
    }

    let mut graph = RuntimeGraph {
        chains: HashMap::new(),
    };
    let mut chain = two_device_chain("chain:2dev", 100.0);
    graph
        .upsert_chain(
            &chain,
            48_000.0,
            &HashMap::new(),
            true,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_two_device(),
        )
        .unwrap();

    // The cpal callbacks capture these Arcs at stream-build time and keep
    // them until the stream is rebuilt (which a volume edit does NOT do).
    let held: Vec<Arc<super::ChainRuntimeState>> = graph.runtimes_for(&chain.id);
    assert!(held.len() >= 2, "fixture: expected ≥2 per-input runtimes");

    // User drags volume → 150. Same call the controller makes for a live
    // chain whose stream signature is unchanged: needs_stream_rebuild=false.
    chain.volume = 150.0;
    graph
        .upsert_chain(
            &chain,
            48_000.0,
            &HashMap::new(),
            false,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_two_device(),
        )
        .unwrap();

    for (i, rt) in held.iter().enumerate() {
        assert_eq!(
            rt.volume_pct(),
            150.0,
            "runtime #{i} held by the live cpal callback still reads the OLD \
             volume — multi-input volume edit never reaches the audio thread"
        );
    }
}


#[test]
fn poll_errors_drains_and_returns_all() {
    let chain = tuner_track("chain:0", Vec::new());
    let runtime =
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &[]).unwrap();
    // Manually push errors
    runtime
        .error_queue
        .push(BlockError {
            block_id: BlockId("err:1".into()),
            message: "oops".into(),
        })
        .unwrap();
    runtime
        .error_queue
        .push(BlockError {
            block_id: BlockId("err:2".into()),
            message: "boom".into(),
        })
        .unwrap();
    let errors = runtime.poll_errors();
    assert_eq!(errors.len(), 2);
    // Second call should be empty
    let errors2 = runtime.poll_errors();
    assert!(errors2.is_empty(), "poll_errors should drain the queue");
}


#[test]
fn split_chain_with_insert_produces_two_segments() {
    let chain = insert_chain();
    let (resolved_in, resolved_out) =
        crate::runtime_endpoints::resolve_chain_io(&chain, &insert_registry());
    let (eff_inputs, cpal_indices, split_positions, entry_groups) =
        effective_inputs(&chain, &resolved_in, &insert_registry());
    let eff_outputs = effective_outputs(&chain, &resolved_out, &insert_registry());
    let segments = split_chain_into_segments(
        &chain,
        &eff_inputs,
        &cpal_indices,
        &split_positions,
        &entry_groups,
        &eff_outputs,
        &insert_registry(),
    );

    // Should have 2 segments: before insert and after insert
    assert_eq!(
        segments.len(),
        2,
        "insert should split chain into 2 segments"
    );

    // Segment 0: input → [comp:0] → insert send
    assert_eq!(
        segments[0].block_indices,
        vec![1],
        "first segment should contain only effect blocks before insert"
    );

    // Segment 1: insert return → [delay:0] → output
    assert_eq!(
        segments[1].block_indices,
        vec![3],
        "second segment should contain only effect blocks after insert"
    );
}


#[test]
fn split_chain_with_disabled_insert_produces_one_segment() {
    let mut chain = insert_chain();
    // Disable the insert block
    chain.blocks[2].enabled = false;
    let (resolved_in, resolved_out) =
        crate::runtime_endpoints::resolve_chain_io(&chain, &insert_registry());
    let (eff_inputs, cpal_indices, split_positions, entry_groups) =
        effective_inputs(&chain, &resolved_in, &insert_registry());
    let eff_outputs = effective_outputs(&chain, &resolved_out, &insert_registry());
    let segments = split_chain_into_segments(
        &chain,
        &eff_inputs,
        &cpal_indices,
        &split_positions,
        &entry_groups,
        &eff_outputs,
        &insert_registry(),
    );

    // Disabled insert should not split the chain
    assert_eq!(
        segments.len(),
        1,
        "disabled insert should not split the chain"
    );
}


// ── #716: per-binding routing (no cross-binding) ──────────────────────────

#[test]
fn split_two_bindings_pairs_each_input_with_its_own_binding_output() {
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    fn b(id: &str, dev: &str) -> IoBinding {
        IoBinding {
            id: id.into(),
            name: id.into(),
            inputs: vec![IoEndpoint {
                name: "In".into(),
                device_id: DeviceId(format!("{dev}-in")),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
            outputs: vec![IoEndpoint {
                name: "Out".into(),
                device_id: DeviceId(format!("{dev}-out")),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
        }
    }
    let reg = vec![b("scarlet", "scarlett"), b("teyun", "teyun")];
    let mut chain = empty_chain("rig:input-1");
    chain.io_binding_ids = vec!["scarlet".into(), "teyun".into()];

    let (rin, rout) = crate::runtime_endpoints::resolve_chain_io(&chain, &reg);
    let (eff_in, cpal, splits, groups) = effective_inputs(&chain, &rin, &reg);
    let eff_out = effective_outputs(&chain, &rout, &reg);
    let segments =
        split_chain_into_segments(&chain, &eff_in, &cpal, &splits, &groups, &eff_out, &reg);

    let scarlet_out = eff_out
        .iter()
        .position(|o| o.device_id.0 == "scarlett-out")
        .unwrap();

    // No cross-routing: the TEYUN input must never reach the SCARLET output.
    assert!(
        !segments
            .iter()
            .any(|s| s.input.device_id.0 == "teyun-in"
                && s.output_route_indices.contains(&scarlet_out)),
        "cross-binding routing: TEYUN input reached the SCARLET output"
    );
    // Sanity: the TEYUN input still routes somewhere (to its own output).
    assert!(
        segments.iter().any(|s| s.input.device_id.0 == "teyun-in"),
        "the TEYUN input must still produce a segment"
    );
}


// ── runtime graph with multiple chains ──────────────────────────────────

#[test]
fn build_runtime_graph_with_multiple_enabled_chains() {
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![
            tuner_track("chain:0", Vec::new()),
            tuner_track("chain:1", vec![tuner_block("b:0", 440.0)]),
        ],
        midi: None,
    };
    let mut rates = HashMap::new();
    rates.insert(ChainId("chain:0".into()), 48_000.0);
    rates.insert(ChainId("chain:1".into()), 48_000.0);

    let runtime = build_runtime_graph(&project, &rates, &HashMap::new(), &[])
        .expect("should build with multiple chains");
    assert_eq!(runtime.chains.len(), 2);
}


#[test]
fn build_runtime_graph_mixed_enabled_and_disabled() {
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![
            Chain {
                id: ChainId("disabled".into()),
                description: None,
                instrument: "electric_guitar".to_string(),
                enabled: false,
                volume: 100.0,
                io_binding_ids: vec![],
                blocks: vec![],
                di_output: None,
            },
            tuner_track("enabled", vec![tuner_block("b:0", 440.0)]),
        ],
        midi: None,
    };
    let mut rates = HashMap::new();
    rates.insert(ChainId("enabled".into()), 48_000.0);

    let runtime = build_runtime_graph(&project, &rates, &HashMap::new(), &[]).unwrap();
    assert_eq!(runtime.chains.len(), 1);
    assert!(!runtime.runtimes_for(&ChainId("enabled".into())).is_empty());
}

