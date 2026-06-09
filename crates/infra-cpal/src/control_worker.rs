//! Issue #672 — off-thread control plane.
//!
//! `ControlWorker` owns a single dedicated thread that runs heavy chain-runtime
//! builds. The frontend thread (which owns the `!Send` dispatcher) calls
//! [`ControlWorker::submit`], which only enqueues the build closure and returns
//! immediately with a `Receiver` for the result — it never runs the build
//! inline. This keeps the Slint event loop responsive while CPAL streams and
//! the runtime graph are (re)built off-thread (the freeze in issue #672).
//!
//! A single worker (not a pool) so rebuilds serialise and a later rebuild for a
//! chain supersedes an in-flight one deterministically. The expensive *drop* of
//! a superseded runtime also happens on this thread — never on the audio or
//! frontend thread.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

/// A unit of work for the control worker: it produces its result and delivers
/// it to the caller's receiver, then returns. Boxed so jobs with different
/// result types share one queue.
type Job = Box<dyn FnOnce() + Send + 'static>;

/// Owns the dedicated control-plane worker thread.
///
/// Dropping the `ControlWorker` closes the job queue; the worker drains any
/// already-enqueued jobs and then exits, and the drop joins it so no build
/// outlives the controller.
pub struct ControlWorker {
    tx: Option<Sender<Job>>,
    handle: Option<JoinHandle<()>>,
}

impl ControlWorker {
    /// Spawn the worker thread.
    #[must_use]
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        let handle = std::thread::Builder::new()
            .name("openrig-control-worker".to_string())
            .spawn(move || {
                // Runs each job in submission order until the sender is dropped.
                for job in rx {
                    job();
                }
            })
            .expect("spawn control worker thread");
        Self {
            tx: Some(tx),
            handle: Some(handle),
        }
    }

    /// Enqueue `build` to run on the worker thread and return a `Receiver` for
    /// its result. Returns immediately — `build` does NOT run on the caller.
    pub fn submit<T, F>(&self, build: F) -> Receiver<T>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        let (result_tx, result_rx) = mpsc::channel::<T>();
        let job: Job = Box::new(move || {
            let value = build();
            // Receiver may have been dropped if the caller lost interest; that
            // is fine — the build already ran (and any drop happened here).
            let _ = result_tx.send(value);
        });
        self.tx
            .as_ref()
            .expect("control worker sender is live until drop")
            .send(job)
            .expect("control worker thread is alive");
        result_rx
    }
}

impl Default for ControlWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ControlWorker {
    fn drop(&mut self) {
        // Close the queue so the worker loop ends, then join it.
        self.tx = None;
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
