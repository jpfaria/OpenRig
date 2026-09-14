//! #934 — dropping the control plane must not block the caller (frontend
//! thread) while a build is still running on the worker.
//!
//! Switching the LAST enabled chain off drops the `ProjectRuntimeController`,
//! and with it the `ControlWorker`. Its drop used to JOIN the worker thread —
//! so a switch-off that landed while a build (CoreAudio device resolve + the
//! NAM/IR load, seconds on the owner's rig) was still in flight froze the GUI
//! until that build finished. Deterministic, like `control_worker_nonblocking`:
//! the job parks on a channel the test controls, the worker is dropped on a
//! helper thread, and the drop must report back BEFORE the job is released.

use std::sync::mpsc;
use std::time::Duration;

use infra_cpal::ControlWorker;

#[test]
fn drop_returns_while_the_build_is_still_running() {
    let worker = ControlWorker::new();

    let (release_tx, release_rx) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let _result_rx = worker.submit(move || {
        started_tx.send(()).expect("signal start");
        release_rx.recv().expect("await release");
    });
    started_rx
        .recv()
        .expect("worker must start the build on its own thread");

    // Drop on a helper so a blocking drop shows up as a timeout, not a hang.
    let (dropped_tx, dropped_rx) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        drop(worker);
        let _ = dropped_tx.send(());
    });

    let dropped = dropped_rx.recv_timeout(Duration::from_secs(3));
    // Release the parked build either way so the helper never leaks.
    let _ = release_tx.send(());
    assert!(
        dropped.is_ok(),
        "dropping the ControlWorker waited for the build in flight — the frontend thread freezes for the whole build"
    );
}
