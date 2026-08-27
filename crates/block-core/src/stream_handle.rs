//! Responsibility: publishes a processor's live readings to the GUI without locking.

use arc_swap::ArcSwap;
use std::sync::Arc;

/// A single key-value entry in a real-time data stream.
/// Any block can publish stream entries for the GUI to display.
#[derive(Debug, Clone)]
pub struct StreamEntry {
    pub key: String,
    pub value: f32,
    pub text: String,
    /// Peak hold level (0.0–1.0). Used by spectrum-type streams; 0.0 for others.
    pub peak: f32,
}

/// Shared handle for publishing stream data from a processor to the GUI.
///
/// Wait-free on both sides: the producer (block worker thread) does
/// `stream.store(Arc::new(new_entries))` to publish a snapshot, and the
/// GUI does `stream.load()` to read the latest snapshot atomically. No
/// `Mutex`, no contention, no priority inversion. The producer's
/// `Arc::new(...)` allocation is acceptable because it runs on a worker
/// thread (e.g. `tuner-detection`, `spectrum-analyzer`) that the RT
/// audio callback only feeds via a bounded channel — never on the RT
/// callback path itself.
pub type StreamHandle = Arc<ArcSwap<Vec<StreamEntry>>>;

/// Construct a fresh, empty `StreamHandle`. Use this in block builders
/// instead of `Arc::new(Mutex::new(Vec::new()))`.
pub fn new_stream_handle() -> StreamHandle {
    Arc::new(ArcSwap::from_pointee(Vec::new()))
}
