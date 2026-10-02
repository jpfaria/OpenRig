//! Responsibility: hands shared values to the audio thread without freeing them there.
//!
//! The control side `send`s an `Arc`; the audio callback `take_latest`s it
//! through a bounded lock-free queue. Freeing memory on the audio thread is a
//! syscall waiting to happen, so the control side keeps its own clone of
//! everything it ever sent: when the callback drops a value it replaced, it
//! only decrements a counter. `collect`, on the control side, releases the
//! values nobody else holds any more.
//!
//! The `Mutex` guards the control side's list only. The audio thread never
//! touches it — its whole API is `take_latest`, a lock-free queue pop.

use std::sync::{Arc, Mutex, MutexGuard};

use crossbeam_queue::ArrayQueue;

/// Values waiting for the callback; a newer send displaces the oldest.
const QUEUE_CAPACITY: usize = 4;

pub struct ArcHandoff<T> {
    queue: ArrayQueue<Arc<T>>,
    held: Mutex<Held<T>>,
}

/// What the control side keeps alive for the callback.
struct Held<T> {
    /// The last value sent; never collected.
    latest: Option<Arc<T>>,
    /// Values sent earlier or retained, until only this list holds them.
    others: Vec<Arc<T>>,
}

impl<T> Held<T> {
    fn collect(&mut self) {
        self.others.retain(|value| Arc::strong_count(value) > 1);
    }
}

impl<T> Default for ArcHandoff<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ArcHandoff<T> {
    pub fn new() -> Self {
        Self {
            queue: ArrayQueue::new(QUEUE_CAPACITY),
            held: Mutex::new(Held {
                latest: None,
                others: Vec::new(),
            }),
        }
    }

    /// Control side: queue `value` for the callback.
    pub fn send(&self, value: Arc<T>) {
        let mut held = self.held();
        held.collect();
        if let Some(previous) = held.latest.replace(Arc::clone(&value)) {
            held.others.push(previous);
        }
        // A displaced value is dropped here, on the control side.
        drop(self.queue.force_push(value));
    }

    /// Control side: keep a value the callback received some other way (at
    /// build time) so the callback never holds its last reference.
    pub fn retain(&self, value: Arc<T>) {
        self.held().others.push(value);
    }

    /// Control side: the last value sent.
    pub fn latest(&self) -> Option<Arc<T>> {
        self.held().latest.clone()
    }

    /// Control side: release every value only this handoff still holds.
    pub fn collect(&self) {
        self.held().collect();
    }

    /// Audio side: the newest value sent since the last call. Never allocates
    /// and never frees: every value it returns or drops is also held by the
    /// control side.
    pub fn take_latest(&self) -> Option<Arc<T>> {
        let mut latest = None;
        while let Some(value) = self.queue.pop() {
            latest = Some(value);
        }
        latest
    }

    fn held(&self) -> MutexGuard<'_, Held<T>> {
        self.held.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
#[path = "arc_handoff_tests.rs"]
mod tests;
