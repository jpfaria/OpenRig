//! #934 — switching the last enabled chain off must not wait for the build
//! that chain still has in flight.
//!
//! The owner's report: "when I switch a chain off, OpenRig freezes". The
//! sequence: the chain's activation (or a live rebuild after a knob turn) is
//! still building on the control worker; the switch-off cancels it (#929) and
//! leaves nothing running, so the frontend drops the controller — whose
//! `ControlWorker` joined its thread on drop, parking the GUI until the build
//! finished. Deterministic: the build parks on a channel the test controls and
//! is released only AFTER the drop has returned (or after a generous timeout,
//! which is the failure).

use std::sync::mpsc;
use std::time::Duration;

use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;

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

#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn switching_the_last_chain_off_returns_before_its_build_finishes() {
    let chain_id = ChainId("rig:input-1".into());
    let mut controller = ProjectRuntimeController::for_testing(engine::runtime::RuntimeGraph {
        chains: std::collections::HashMap::new(),
    });

    // The chain was switched on: its build is on the worker, still running.
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let rx = controller.worker.submit(move || {
        started_tx.send(()).expect("signal start");
        release_rx.recv().expect("await release");
        Err(anyhow::anyhow!("build released by the test"))
    });
    controller
        .pending_activations
        .push((chain_id.clone(), chain(&chain_id.0, true), rx));
    controller.streams.activation_started(&chain_id);
    started_rx
        .recv()
        .expect("worker must start the build on its own thread");

    // The user switches the chain off: the frontend's SwitchOff path.
    controller
        .upsert_chain(&project(), &chain(&chain_id.0, false))
        .expect("switching off never fails");
    assert!(
        !controller.is_running(),
        "nothing runs any more — the frontend drops the controller now"
    );

    // The helper releases the build only once the drop reported back — or
    // after a generous timeout, which is the freeze this test pins.
    let (dropped_tx, dropped_rx) = mpsc::channel::<()>();
    let (verdict_tx, verdict_rx) = mpsc::channel::<bool>();
    std::thread::spawn(move || {
        let returned = dropped_rx.recv_timeout(Duration::from_secs(3)).is_ok();
        let _ = release_tx.send(());
        let _ = verdict_tx.send(returned);
    });

    drop(controller);
    let _ = dropped_tx.send(());

    let returned_before_release = verdict_rx.recv().expect("helper verdict");
    assert!(
        returned_before_release,
        "dropping the controller waited for the chain's build in flight — the GUI freezes for the whole build"
    );
}

/// The second freeze behind #934, from the owner's `sample` of the hung app:
/// the GUI thread inside the LV2 plugin's `cleanup` (TAL-Filter-2, a JUCE
/// plugin) — `kill_chain_streams` dropped the LAST reference to the chain's
/// runtime on the frontend thread. A JUCE plugin built on the control worker
/// waits, on cleanup, for the message thread it was created on; run that
/// cleanup on the GUI thread and it waits forever. The rule the rebuild path
/// already follows: a runtime is dropped on the control worker, never on the
/// frontend thread. Deterministic: the worker is parked, so whoever frees the
/// runtime before the release is the frontend thread.
#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn switching_a_chain_off_frees_its_runtime_on_the_worker_not_on_the_caller() {
    use super::active_runtime::ActiveChainRuntime;
    use super::resolved::ChainStreamSignature;

    let chain_id = ChainId("rig:input-1".into());
    let on = chain(&chain_id.0, true);
    let runtime = std::sync::Arc::new(
        engine::runtime::build_chain_runtime_state(&on, 48_000.0, &[1024], &[])
            .expect("empty chain runtime should build"),
    );
    let weak = std::sync::Arc::downgrade(&runtime);
    let mut graph = engine::runtime::RuntimeGraph {
        chains: std::collections::HashMap::new(),
    };
    graph.chains.insert((chain_id.clone(), 0), runtime);
    // `for_testing` mirrors production: one live slot per graph entry.
    let mut controller = ProjectRuntimeController::for_testing(graph);
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
        },
    );
    controller.streams.streams_built(&chain_id, 1, 1, 2);

    // Park the worker: nothing it is handed can be dropped until released.
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let _parked = controller.worker.submit(move || {
        started_tx.send(()).expect("signal start");
        release_rx.recv().expect("await release");
    });
    started_rx.recv().expect("worker parked");

    controller
        .upsert_chain(&project(), &chain(&chain_id.0, false))
        .expect("switching off never fails");

    assert!(
        weak.upgrade().is_some(),
        "the frontend thread freed the chain's runtime itself — a plugin cleanup on the GUI thread (the TAL-Filter-2 freeze)"
    );

    release_tx.send(()).expect("release the worker");
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while weak.upgrade().is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "the worker never freed the runtime the switch-off handed it"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Stopping the rig / closing the project drops the controller on the
/// frontend thread; the chain runtimes (and their plugins) must still be
/// freed on the worker, which outlives the drop.
#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn dropping_the_controller_frees_the_runtimes_on_the_worker_not_on_the_caller() {
    let chain_id = ChainId("rig:input-1".into());
    let runtime = std::sync::Arc::new(
        engine::runtime::build_chain_runtime_state(
            &chain(&chain_id.0, true),
            48_000.0,
            &[1024],
            &[],
        )
        .expect("empty chain runtime should build"),
    );
    let weak = std::sync::Arc::downgrade(&runtime);
    let mut graph = engine::runtime::RuntimeGraph {
        chains: std::collections::HashMap::new(),
    };
    graph.chains.insert((chain_id.clone(), 0), runtime);
    let controller = ProjectRuntimeController::for_testing(graph);

    let (release_tx, release_rx) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let _parked = controller.worker.submit(move || {
        started_tx.send(()).expect("signal start");
        release_rx.recv().expect("await release");
    });
    started_rx.recv().expect("worker parked");

    drop(controller);

    assert!(
        weak.upgrade().is_some(),
        "the frontend thread freed the rig's runtime itself while dropping the controller"
    );

    release_tx.send(()).expect("release the worker");
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while weak.upgrade().is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "the worker never freed the runtimes the controller handed it"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
