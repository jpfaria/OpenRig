//! Responsibility: swaps a chain's live runtime without waiting.
//! Issue #672 — wait-free swappable holder of a chain's live runtime.
//!
//! The audio callback calls [`LiveRuntimeSlot::load`] once per buffer to obtain
//! the current `ChainRuntimeState`. The load is wait-free (`arc-swap`): zero
//! lock, zero alloc, zero syscall on the audio thread (invariant #8). The
//! control worker calls [`LiveRuntimeSlot::publish`] to install a rebuilt
//! runtime and gets the previous `Arc` back, so the superseded runtime is
//! dropped on the worker thread — never on the audio thread.

use std::sync::Arc;
use std::time::Instant;

use arc_swap::ArcSwap;
use engine::runtime::ChainRuntimeState;

use crate::slot_handover::SlotHandover;

struct SlotCell {
    live: ArcSwap<ChainRuntimeState>,
    handover: SlotHandover,
}

/// A wait-free, swappable handle to a single chain's live runtime.
///
/// Clone the handle with [`LiveRuntimeSlot::handle`]: the audio callback and the
/// control worker share one underlying slot, so a `publish` from the worker is
/// observed by the callback's next `load`.
pub struct LiveRuntimeSlot(Arc<SlotCell>);

impl LiveRuntimeSlot {
    /// Create a slot already holding `initial`.
    #[must_use]
    pub fn new(initial: Arc<ChainRuntimeState>) -> Self {
        // #980: the memory a live runtime touches must be wired right away.
        crate::memory_residency_keeper::wire_soon();
        Self(Arc::new(SlotCell {
            live: ArcSwap::from(initial),
            handover: SlotHandover::new(),
        }))
    }

    /// Audio-thread read: wait-free load of the current runtime.
    #[must_use]
    pub fn load(&self) -> Arc<ChainRuntimeState> {
        self.0.live.load_full()
    }

    /// Worker-thread publish: install `next`, returning the previous runtime so
    /// the caller drops it off the audio thread.
    #[must_use]
    pub fn publish(&self, next: Arc<ChainRuntimeState>) -> Arc<ChainRuntimeState> {
        crate::memory_residency_keeper::wire_soon();
        self.0.live.swap(next)
    }

    /// Worker-thread publish of a live edit (#987): install `next` while the
    /// runtime it replaces keeps playing, until `next` has warmed up and every
    /// output has crossfaded to it. [`LiveRuntimeSlot::reap`] releases it.
    pub fn hand_over(&self, next: Arc<ChainRuntimeState>) {
        crate::memory_residency_keeper::wire_soon();
        let previous = self.0.live.swap(next);
        self.0.handover.begin(previous);
    }

    /// Control side: the replaced runtimes that are done and no longer held
    /// by any audio thread, for the caller to drop off the audio thread.
    #[must_use]
    pub fn reap(&self, now: Instant) -> Vec<Arc<ChainRuntimeState>> {
        self.0.handover.reap(now)
    }

    pub(crate) fn handover(&self) -> &SlotHandover {
        &self.0.handover
    }

    /// Cheap clone of the handle — the new handle shares the same slot.
    #[must_use]
    pub fn handle(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl Clone for LiveRuntimeSlot {
    /// Cloning shares the same underlying slot (same as [`LiveRuntimeSlot::handle`]).
    fn clone(&self) -> Self {
        self.handle()
    }
}
