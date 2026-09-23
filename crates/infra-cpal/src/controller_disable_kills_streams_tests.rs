//! #929 — switching a chain off kills EVERY stream it owns, open or on its
//! way. The owner's rule: the controller keeps an in-memory index chain →
//! streams, and "off" reads it and takes everything down — the open device
//! streams AND every activation still building — so nothing lands afterwards
//! and plays a chain the screen shows as off (the log that opened the issue:
//! `pausing chain 'rig:input-1'` at :47, `audio streams started for chain
//! 'rig:input-1'` at :52, :57 and :02).

use std::sync::Arc;

use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;

use super::active_runtime::ActiveChainRuntime;
use super::resolved::ChainStreamSignature;
use super::ProjectRuntimeController;

fn chain(id: &str, enabled: bool) -> Chain {
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

fn project() -> Project {
    Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }
}

fn controller() -> ProjectRuntimeController {
    ProjectRuntimeController {
        runtime_graph: engine::runtime::RuntimeGraph {
            chains: std::collections::HashMap::new(),
        },
        active_chains: std::collections::HashMap::new(),
        chain_slots: std::collections::HashMap::new(),
        worker: crate::ControlWorker::new(),
        pending_rebuilds: Vec::new(),
        pending_activations: Vec::new(),
        streams: Default::default(),
        toggle_replays: Default::default(),
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
    }
}

/// A chain whose streams are open: one runtime in the graph, one active entry,
/// and the registry saying it owns 1 input + 2 output streams.
fn controller_with_open_streams(
    chain_id: &ChainId,
) -> (
    ProjectRuntimeController,
    Arc<engine::runtime::ChainRuntimeState>,
) {
    let mut controller = controller();
    let on = chain(&chain_id.0, true);
    let runtime = Arc::new(
        engine::runtime::build_chain_runtime_state(&on, 48_000.0, &[1024], &[])
            .expect("empty chain runtime should build"),
    );
    controller
        .runtime_graph
        .chains
        .insert((chain_id.clone(), 0), Arc::clone(&runtime));
    controller.active_chains.insert(
        chain_id.clone(),
        ActiveChainRuntime {
            resolved: None,
            structure: Vec::new(),
            generation: 1,
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
    controller.streams.streams_built(chain_id, 1, 1, 2);
    (controller, runtime)
}

#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn switching_a_chain_off_cancels_the_activation_still_building() {
    let chain_id = ChainId("rig:input-1".into());
    let mut controller = controller();
    // The user switched the chain on: the worker is building it.
    let (tx, rx) = std::sync::mpsc::channel();
    controller
        .pending_activations
        .push((chain_id.clone(), chain(&chain_id.0, true), rx));
    controller.streams.activation_started(&chain_id);
    assert!(
        controller.streams.owns(&chain_id),
        "the build in flight is owned"
    );

    // …and switched it off before the build landed.
    controller
        .upsert_chain(&project(), &chain(&chain_id.0, false))
        .expect("switching off succeeds");

    assert!(
        controller.pending_activations.is_empty(),
        "#929: the activation in flight must be cancelled — it was left to land \
         after the pause and open streams for a chain shown off"
    );
    assert!(
        tx.send(Err(anyhow::anyhow!("late build"))).is_err(),
        "the worker's result has nowhere to land: the receiver is gone"
    );
    assert!(
        !controller.streams.owns(&chain_id),
        "the index says the chain owns nothing once it is off"
    );
    assert_eq!(controller.poll_pending_rebuilds(), 0, "nothing lands later");
    assert!(controller.active_chains.is_empty());
}

#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn switching_a_chain_off_kills_its_open_streams() {
    let chain_id = ChainId("rig:input-4".into());
    let (mut controller, _runtime) = controller_with_open_streams(&chain_id);
    assert_eq!(
        controller
            .streams
            .owned(&chain_id)
            .map(|o| (o.input_streams, o.output_streams)),
        Some((1, 2)),
        "the index knows the chain owns 1 input + 2 output streams"
    );

    controller
        .upsert_chain(&project(), &chain(&chain_id.0, false))
        .expect("switching off succeeds");

    assert!(
        !controller.active_chains.contains_key(&chain_id),
        "#929: off = the chain's streams are gone, not paused"
    );
    assert!(
        controller.runtime_graph.runtimes_for(&chain_id).is_empty(),
        "no runtime survives the switch-off"
    );
    assert!(
        controller.streams.owned(&chain_id).is_none(),
        "the index has nothing left for the chain"
    );
}

#[test]
fn the_index_counts_streams_and_builds_per_chain() {
    let mut index = crate::ChainStreamRegistry::default();
    let a = ChainId("a".into());
    assert!(!index.owns(&a));

    index.activation_started(&a);
    assert!(index.owns(&a), "a build in flight is ownership");
    index.activation_settled(&a);
    index.streams_built(&a, 3, 1, 2);
    let owned = index.owned(&a).cloned().expect("registered");
    assert_eq!(
        (
            owned.generation,
            owned.input_streams,
            owned.output_streams,
            owned.activations_in_flight
        ),
        (3, 1, 2, 0)
    );

    assert_eq!(index.forget(&a).map(|o| o.output_streams), Some(2));
    assert!(index.owned(&a).is_none());
}
