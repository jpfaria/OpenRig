//! Switching a chain off — the #522 fast path, superseded by #929.
//!
//! #522 swapped `remove_chain` for a pause: the runtime stayed in
//! `active_chains`, the CPAL streams stayed open draining to silence, and
//! re-enabling cleared the flag in O(1). That pause never touched the
//! activations still building, so they landed seconds after the switch-off
//! and opened streams for a chain the screen showed as off (#929). The
//! owner's rule now: off = every stream the chain owns dies, open or in
//! flight; on = a fresh cold activation. These tests pin that contract for
//! the doors #522 and #545 covered (single and multi-input-group chains).

use std::sync::Arc;

use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;

use super::active_runtime::ActiveChainRuntime;
use super::resolved::ChainStreamSignature;
use super::ProjectRuntimeController;

fn empty_chain(id: &str, enabled: bool) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

fn empty_project() -> Project {
    Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }
}

fn controller_with_active_chain(
    chain_id: &ChainId,
) -> (
    ProjectRuntimeController,
    Arc<engine::runtime::ChainRuntimeState>,
) {
    let chain = empty_chain(&chain_id.0, true);
    let runtime_arc = Arc::new(
        engine::runtime::build_chain_runtime_state(&chain, 48_000.0, &[1024], &[])
            .expect("empty chain runtime should build"),
    );

    let mut graph = engine::runtime::RuntimeGraph {
        chains: std::collections::HashMap::new(),
    };
    graph
        .chains
        .insert((chain_id.clone(), 0), Arc::clone(&runtime_arc));

    let mut active_chains = std::collections::HashMap::new();
    active_chains.insert(
        chain_id.clone(),
        ActiveChainRuntime {
            resolved: None,
            structure: Vec::new(),
            generation: 0,
            stream_signature: ChainStreamSignature {
                inputs: vec![],
                outputs: vec![],
            },
            _input_streams: vec![],
            _output_streams: vec![],
            #[cfg(all(target_os = "linux", feature = "jack"))]
            _jack_client: None,
            #[cfg(all(target_os = "linux", feature = "jack"))]
            _dsp_worker: None,
        },
    );

    let controller = ProjectRuntimeController {
        runtime_graph: graph,
        active_chains,
        chain_slots: std::collections::HashMap::new(),
        worker: crate::ControlWorker::new(),
        pending_rebuilds: Vec::new(),
        pending_activations: Vec::new(),
        streams: Default::default(),
        stream_generation: 0,
        sample_rate: 48_000,
        io_bindings: Vec::new(),
        di_streams: std::cell::RefCell::new(std::collections::HashMap::new()),
        di_playback_cells: std::cell::RefCell::new(std::collections::HashMap::new()),
        di_retired: Default::default(),
        looper_armed: std::cell::RefCell::new(std::collections::HashMap::new()),
        looper_store: std::cell::RefCell::new(crate::looper_store::LooperStore::default()),
        metronome_stream: std::cell::RefCell::new(None),
        metronome_shared: std::sync::Arc::new(engine::metronome_state::MetronomeShared::new(
            Default::default(),
        )),
        #[cfg(all(target_os = "linux", feature = "jack"))]
        supervisor: super::jack_supervisor::JackSupervisor::new(
            super::jack_supervisor::LiveJackBackend::new(),
        ),
    };
    (controller, runtime_arc)
}

#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn upsert_chain_disabled_kills_runtime_and_streams() {
    let chain_id = ChainId("chain:929:off".into());
    let (mut controller, runtime_arc) = controller_with_active_chain(&chain_id);
    let project = empty_project();

    assert!(controller.active_chains.contains_key(&chain_id));

    let disabled = empty_chain(&chain_id.0, false);
    controller
        .upsert_chain(&project, &disabled)
        .expect("upsert with enabled=false must succeed");

    assert!(
        !controller.active_chains.contains_key(&chain_id),
        "#929: switching a chain off drops its runtime and streams — nothing \
         of the chain stays open to play behind an off switch"
    );
    assert!(
        runtime_arc.is_draining(),
        "the dropped runtime is drained first so the last callbacks emit silence"
    );
    assert!(
        controller.runtime_graph.runtimes_for(&chain_id).is_empty(),
        "no runtime survives in the graph"
    );
}

#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn upsert_chain_enabled_after_off_is_a_fresh_activation() {
    let chain_id = ChainId("chain:929:on-again".into());
    let (mut controller, _runtime_arc) = controller_with_active_chain(&chain_id);
    let project = empty_project();

    let disabled = empty_chain(&chain_id.0, false);
    controller
        .upsert_chain(&project, &disabled)
        .expect("disable succeeds");
    assert!(controller.active_chains.is_empty());

    // Re-enable: with nothing live, the controller schedules a cold
    // activation — the build runs off-thread and the chain becomes owned
    // again through the index, never by resuming a paused runtime.
    let enabled = empty_chain(&chain_id.0, true);
    let scheduled = controller
        .schedule_chain_activation(&project, &enabled)
        .expect("scheduling the activation succeeds");
    assert!(
        scheduled,
        "a chain with no live streams is activated from scratch"
    );
    assert_eq!(controller.pending_activations.len(), 1);
    assert!(
        controller.streams.owns(&chain_id),
        "the index owns the build in flight"
    );
}

/// Issue #545 kept: a chain with several input groups (one runtime per
/// physical input device, #350 Phase 3) must go down as a whole — every group,
/// not only the first `runtime_for_chain` returns.
#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn switching_off_kills_every_input_group_runtime() {
    let chain_id = ChainId("chain:545:multi-input".into());
    let (mut controller, group0) = controller_with_active_chain(&chain_id);

    let chain = empty_chain(&chain_id.0, true);
    let group1 = Arc::new(
        engine::runtime::build_chain_runtime_state(&chain, 48_000.0, &[1024], &[])
            .expect("group-1 runtime should build"),
    );
    controller
        .runtime_graph
        .chains
        .insert((chain_id.clone(), 1), Arc::clone(&group1));

    let disabled = empty_chain(&chain_id.0, false);
    controller
        .upsert_chain(&empty_project(), &disabled)
        .expect("disable succeeds");

    assert!(
        controller.runtime_graph.runtimes_for(&chain_id).is_empty(),
        "REGRESSION: a group survived the switch-off — the user sees the chain \
         alive (tap moving, CPU not dropping) with the switch off"
    );
    assert!(group0.is_draining(), "group 0 drained on the way out");
}

// ── Issue #670: per-chain xrun count accessor for the GUI overload meter ──

#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn chain_xrun_count_reports_runtime_overruns() {
    let chain_id = ChainId("chain:670:xrun".into());
    let (controller, runtime_arc) = controller_with_active_chain(&chain_id);
    assert_eq!(controller.chain_xrun_count(&chain_id), 0);
    // Two overrunning callbacks (2 ms each against a 1 ms deadline).
    runtime_arc.record_callback_load(2_000_000, 1_000_000);
    runtime_arc.record_callback_load(2_000_000, 1_000_000);
    assert_eq!(controller.chain_xrun_count(&chain_id), 2);
}

#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn chain_xrun_count_is_zero_for_unknown_chain() {
    let chain_id = ChainId("chain:670:known".into());
    let (controller, _rt) = controller_with_active_chain(&chain_id);
    assert_eq!(
        controller.chain_xrun_count(&ChainId("nope".into())),
        0,
        "unknown chain has no runtime, so no xruns"
    );
}

// ── Issue #923: per-output-route stream accounting for `openrig://routes` ──

#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn chain_output_route_stats_lists_every_route_of_every_group() {
    let chain_id = ChainId("chain:923:routes".into());
    let (controller, runtime_arc) = controller_with_active_chain(&chain_id);
    let groups = controller.chain_output_route_stats(&chain_id);
    assert_eq!(groups.len(), 1, "one per-input runtime");
    let (group, routes) = &groups[0];
    assert_eq!(*group, 0);
    assert_eq!(routes.len(), runtime_arc.take_output_route_stats().len());
    assert!(routes.iter().all(|r| r.callbacks == 0));
}

#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn chain_output_route_stats_is_empty_for_unknown_chain() {
    let chain_id = ChainId("chain:923:known".into());
    let (controller, _rt) = controller_with_active_chain(&chain_id);
    assert!(controller
        .chain_output_route_stats(&ChainId("nope".into()))
        .is_empty());
}
