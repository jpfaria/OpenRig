//! Issue #693 — disk persistence off the dispatching thread.
//!
//! `Command` side-effects used to write files inline, so a slow disk
//! (or any stuck sink) froze the calling thread — in practice the GUI
//! thread, since Slint callbacks dispatch synchronously. Handlers now
//! serialize in memory and enqueue the byte payloads here; a single
//! dedicated worker thread performs the writes in submission order
//! (the "goroutine" pattern: thread + channel). One worker preserves
//! write ordering between jobs of the same save.
//!
//! The queue is unbounded on purpose: a save must never be DROPPED
//! (it is user data), and the caller must never BLOCK — memory is the
//! buffer while the disk catches up. Write errors are reported through
//! `log::error!` (the logger itself is non-blocking, see
//! `adapter-gui::logging`).

use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender, SyncSender};
use std::sync::{Mutex, OnceLock};

pub(crate) enum PersistJob {
    /// `create_dir_all(path)` — queued before dependent writes.
    EnsureDir(PathBuf),
    /// `fs::write(path, bytes)`.
    WriteFile(PathBuf, Vec<u8>),
    /// Arbitrary persistence closure (#693: config read-modify-write).
    /// Runs ordered with every other job — the single worker is what
    /// makes concurrent config edits race-free.
    Run(Box<dyn FnOnce() + Send>),
    /// Barrier: ack once every job queued before it has completed.
    Flush(SyncSender<()>),
}

fn sender() -> &'static Mutex<Sender<PersistJob>> {
    static TX: OnceLock<Mutex<Sender<PersistJob>>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = channel::<PersistJob>();
        std::thread::Builder::new()
            .name("persist-worker".into())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    match job {
                        PersistJob::EnsureDir(path) => {
                            if let Err(e) = std::fs::create_dir_all(&path) {
                                log::error!("persist-worker: create_dir_all {path:?} failed: {e}");
                            }
                        }
                        PersistJob::WriteFile(path, bytes) => {
                            if let Err(e) = std::fs::write(&path, &bytes) {
                                log::error!("persist-worker: write {path:?} failed: {e}");
                            }
                        }
                        PersistJob::Run(job) => job(),
                        PersistJob::Flush(ack) => {
                            let _ = ack.send(());
                        }
                    }
                }
            })
            .expect("spawn persist-worker thread");
        Mutex::new(tx)
    })
}

/// Queue a job. Never blocks: the channel is unbounded and the lock
/// only guards a clone-free `send` (microseconds).
pub(crate) fn enqueue(job: PersistJob) {
    let tx = sender().lock().expect("persist-worker sender poisoned");
    if tx.send(job).is_err() {
        log::error!("persist-worker: worker thread is gone; write lost");
    }
}

/// Queue an arbitrary persistence closure (config read-modify-write,
/// adapter-side file effects). Never blocks the caller; runs ordered
/// with all other persistence jobs on the single worker.
pub fn run(job: impl FnOnce() + Send + 'static) {
    enqueue(PersistJob::Run(Box::new(job)));
}

/// Block until every job queued so far has been written. Call on app
/// shutdown (durability) and in save→reload round-trips (tests).
pub fn flush() {
    let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
    enqueue(PersistJob::Flush(ack_tx));
    let _ = ack_rx.recv();
}
