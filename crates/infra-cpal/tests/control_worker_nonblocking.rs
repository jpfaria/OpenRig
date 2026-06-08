//! Issue #672 — the control plane must not block the caller (frontend thread)
//! while a heavy chain runtime is built.
//!
//! Foundation primitive: `ControlWorker` runs a build closure on a dedicated
//! worker thread. `submit` MUST return immediately (it only enqueues), while
//! the closure keeps running on the worker. This test proves the hand-off is
//! off-thread *deterministically* (no timing/sleep): the submitted closure
//! parks on a channel the test controls, and the test asserts `submit` already
//! returned while the closure is still parked. The synchronous `develop`
//! behaviour (build inline on the caller) cannot satisfy this.

use std::sync::mpsc;

use infra_cpal::ControlWorker;

#[test]
fn submit_returns_before_the_build_closure_finishes() {
    let worker = ControlWorker::new();

    // The worker will block inside the closure until the test releases it.
    let (release_tx, release_rx) = mpsc::channel::<()>();
    // The closure signals it has started so the test knows it is on the worker.
    let (started_tx, started_rx) = mpsc::channel::<()>();

    let result_rx = worker.submit(move || {
        started_tx.send(()).expect("signal start");
        // Park until the test thread releases us — proves we run concurrently
        // with the caller, not inline before `submit` returned.
        release_rx.recv().expect("await release");
        42_u32
    });

    // The closure must already be running on the worker...
    started_rx
        .recv()
        .expect("worker must start the build on its own thread");

    // ...and `submit` must have returned to us *before* we release the closure.
    // If `submit` had run the build inline (develop behaviour), we could never
    // reach this line, because the closure is still parked on `release_rx`.
    release_tx.send(()).expect("release the build");

    let value = result_rx
        .recv()
        .expect("worker must deliver the build result");
    assert_eq!(value, 42, "the worker must return the closure's output");
}
