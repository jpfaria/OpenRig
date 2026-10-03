//! #979 — restarting a chain whose input arrives stepped.
//!
//! On the rig the stepped In 1 was cured only by switching the chain off and
//! on. The engine marks the runtime that reads a stepped input; the controller
//! reports which chains are marked and restarts one exactly like that toggle:
//! the marked runtime and its streams go away and a fresh activation is built.
//! Nothing of another chain is touched.

use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;
use project::project::Project;

use engine::runtime::{build_chain_runtime_state, process_input_f32, ChainRuntimeState};

use super::active_runtime::ActiveChainRuntime;
use super::resolved::ChainStreamSignature;
use super::ProjectRuntimeController;

const STEPPED: &[u8] = include_bytes!("../../engine/tests/fixtures/issue_979/in1_stepped.f32");
const CLEAN: &[u8] = include_bytes!("../../engine/tests/fixtures/issue_979/in1_clean.f32");
const BUFFER: usize = 64;

fn samples(raw: &[u8]) -> Vec<f32> {
    raw.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

fn chain(id: &str, enabled: bool) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("issue-979-no-such-device".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("issue-979-no-such-device".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn project(chains: Vec<Chain>) -> Project {
    Project {
        name: None,
        device_settings: vec![],
        chains,
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
        stream_generation: 0,
        sample_rate: 44_100,
        device_settings: Vec::new(),
        io_bindings: registry(),
        di_streams: std::cell::RefCell::new(std::collections::HashMap::new()),
        di_playback_cells: std::cell::RefCell::new(std::collections::HashMap::new()),
        di_retired: Default::default(),
        looper_armed: std::cell::RefCell::new(std::collections::HashMap::new()),
        looper_store: std::cell::RefCell::new(crate::looper_store::LooperStore::default()),
        metronome_stream: std::cell::RefCell::new(None),
        metronome_shared: std::sync::Arc::new(engine::metronome_state::MetronomeShared::new(
            Default::default(),
        )),
        drums: Default::default(),
        #[cfg(all(target_os = "linux", feature = "jack"))]
        supervisor: super::jack_supervisor::JackSupervisor::new(
            super::jack_supervisor::LiveJackBackend::new(),
        ),
    }
}

/// Puts a running chain in the controller — its runtime in the graph, its
/// active entry, and its streams in the index — and plays `signal` into it
/// the way the device callback would.
fn run_chain(
    controller: &mut ProjectRuntimeController,
    id: &str,
    signal: &[f32],
) -> Arc<ChainRuntimeState> {
    let chain_id = ChainId(id.into());
    let runtime = Arc::new(
        build_chain_runtime_state(&chain(id, true), 44_100.0, &[BUFFER], &registry())
            .expect("the chain runtime builds"),
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
    controller.streams.streams_built(&chain_id, 1, 1, 1);
    for buffer in signal.chunks_exact(BUFFER) {
        process_input_f32(&runtime, 0, buffer, 1);
    }
    runtime
}

#[test]
fn the_chain_fed_the_recorded_broken_input_is_reported_stepped() {
    let mut controller = controller();
    run_chain(&mut controller, "rig:input-1", &samples(STEPPED));
    run_chain(&mut controller, "rig:input-2", &samples(CLEAN));
    assert_eq!(
        controller.stepped_input_chains(),
        vec![ChainId("rig:input-1".into())]
    );
}

#[test]
fn a_rig_fed_clean_input_reports_no_stepped_chain() {
    let mut controller = controller();
    run_chain(&mut controller, "rig:input-1", &samples(CLEAN));
    assert!(controller.stepped_input_chains().is_empty());
}

#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn restarting_a_stepped_chain_rebuilds_it_like_the_toggle() {
    let mut controller = controller();
    let broken = run_chain(&mut controller, "rig:input-1", &samples(STEPPED));
    let id = ChainId("rig:input-1".into());
    let rig = project(vec![chain("rig:input-1", true)]);

    let restarted = controller
        .restart_chain_streams(&rig, &id)
        .expect("the restart is accepted");

    assert!(restarted, "an enabled chain in the project is restarted");
    assert!(
        !controller
            .runtime_graph
            .runtimes_for(&id)
            .iter()
            .any(|rt| Arc::ptr_eq(rt, &broken)),
        "the marked runtime must be gone, as after switching the chain off"
    );
    assert!(
        controller
            .pending_activations
            .iter()
            .any(|(chain_id, _, _)| *chain_id == id),
        "a fresh activation must be on its way, as after switching it on"
    );
    assert!(
        controller.stepped_input_chains().is_empty(),
        "nothing left reports the old stepped state"
    );
}

#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn restarting_one_chain_leaves_another_chain_running() {
    let mut controller = controller();
    run_chain(&mut controller, "rig:input-1", &samples(STEPPED));
    let other = run_chain(&mut controller, "rig:input-2", &samples(CLEAN));
    let rig = project(vec![chain("rig:input-1", true), chain("rig:input-2", true)]);

    controller
        .restart_chain_streams(&rig, &ChainId("rig:input-1".into()))
        .expect("the restart is accepted");

    let other_id = ChainId("rig:input-2".into());
    assert!(
        controller
            .runtime_graph
            .runtimes_for(&other_id)
            .iter()
            .any(|rt| Arc::ptr_eq(rt, &other)),
        "the other chain keeps its runtime"
    );
    assert!(controller.active_chains.contains_key(&other_id));
    assert!(
        !controller
            .pending_activations
            .iter()
            .any(|(chain_id, _, _)| *chain_id == other_id),
        "the other chain is not rebuilt"
    );
}

#[test]
fn a_chain_that_is_off_is_not_restarted() {
    let mut controller = controller();
    let broken = run_chain(&mut controller, "rig:input-1", &samples(STEPPED));
    let id = ChainId("rig:input-1".into());
    let rig = project(vec![chain("rig:input-1", false)]);

    let restarted = controller
        .restart_chain_streams(&rig, &id)
        .expect("the call itself succeeds");

    assert!(!restarted, "a chain switched off is left off");
    assert!(controller.pending_activations.is_empty());
    assert!(controller
        .runtime_graph
        .runtimes_for(&id)
        .iter()
        .any(|rt| Arc::ptr_eq(rt, &broken)));
}
