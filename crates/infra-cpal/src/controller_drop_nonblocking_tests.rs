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
