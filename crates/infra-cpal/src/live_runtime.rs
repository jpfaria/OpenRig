//! Issue #672 — wait-free swappable holder of a chain's live runtime.
//!
//! The audio callback calls [`LiveRuntimeSlot::load`] once per buffer to obtain
//! the current `ChainRuntimeState`. The load is wait-free (`arc-swap`): zero
//! lock, zero alloc, zero syscall on the audio thread (invariant #8). The
//! control worker calls [`LiveRuntimeSlot::publish`] to install a rebuilt
//! runtime and gets the previous `Arc` back, so the superseded runtime is
//! dropped on the worker thread — never on the audio thread.

use std::sync::Arc;

use arc_swap::ArcSwap;
use engine::runtime::ChainRuntimeState;

/// A wait-free, swappable handle to a single chain's live runtime.
///
/// Clone the handle with [`LiveRuntimeSlot::handle`]: the audio callback and the
/// control worker share one underlying slot, so a `publish` from the worker is
/// observed by the callback's next `load`.
pub struct LiveRuntimeSlot(Arc<ArcSwap<ChainRuntimeState>>);

impl LiveRuntimeSlot {
    /// Create a slot already holding `initial`.
    #[must_use]
    pub fn new(initial: Arc<ChainRuntimeState>) -> Self {
        Self(Arc::new(ArcSwap::from(initial)))
    }

    /// Audio-thread read: wait-free load of the current runtime.
    #[must_use]
    pub fn load(&self) -> Arc<ChainRuntimeState> {
        self.0.load_full()
    }

    /// Worker-thread publish: install `next`, returning the previous runtime so
    /// the caller drops it off the audio thread.
    #[must_use]
    pub fn publish(&self, next: Arc<ChainRuntimeState>) -> Arc<ChainRuntimeState> {
        self.0.swap(next)
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
