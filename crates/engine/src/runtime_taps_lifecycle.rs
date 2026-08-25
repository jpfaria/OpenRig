//! Responsibility: keeps a chain's taps alive across a rebuild.
//!
//! Split out of `runtime_state_taps.rs` (#873).

use crate::input_tap::InputTap;
use crate::runtime_state::ChainRuntimeState;
use crate::stream_tap::StreamTap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

impl ChainRuntimeState {
    /// Drop input taps that no longer have any external `SpscRing` handles
    /// kept by consumers. Cheap to call; intended for periodic cleanup
    /// from a UI timer (e.g. when the tuner window closes).
    ///
    /// Detection works because the audio thread only borrows the rings via
    /// the `Arc<InputTap>`; if no consumer holds a handle, the channel
    /// `Arc`s have refcount 1 (only the `InputTap` holds them).
    pub fn prune_dead_input_taps(&self) {
        let current = self.input_taps.load_full();
        let mut kept: Vec<Arc<InputTap>> = Vec::with_capacity(current.len());
        let mut changed = false;
        for tap in current.iter() {
            let has_consumer = tap
                .channel_rings
                .iter()
                .filter_map(|r| r.as_ref())
                .any(|ring| Arc::strong_count(ring) > 1);
            if has_consumer {
                kept.push(Arc::clone(tap));
            } else {
                changed = true;
            }
        }
        if changed {
            self.input_taps.store(Arc::new(kept));
        }
    }

    /// Issue #740: migrate the live tap subscriptions (meter / spectrum /
    /// tuner rings) from a SUPERSEDED runtime onto this freshly-rebuilt one.
    ///
    /// An off-thread rebuild (preset switch, param/block edit) builds a NEW
    /// `ChainRuntimeState` and swaps it into the live slot. The UI subscribed
    /// its taps on the OLD runtime, so without this the rebuilt runtime — now
    /// the one processing audio — feeds nothing and the graph freezes. The taps
    /// are `Arc`s shared with the UI consumers, so adopting the same `Arc`s makes
    /// the new runtime feed the exact rings the UI is already reading. Lock-free
    /// `ArcSwap` store, same as `subscribe_*`.
    /// #749: the DI loop is the same kind of live, runtime-only state as the
    /// taps — armed on the runtime, read by the audio thread, never persisted.
    /// An off-thread rebuild that adopts the taps but NOT the armed loop leaves
    /// the rebuilt (now-live) runtime playing device input while the UI, still
    /// reading the old runtime's `has_di_loop`, shows the loop as playing: the
    /// "icon blue but silent" bug. Carry the loop `Arc` AND its playback cursor
    /// so a loop that was mid-playback resumes from where it was, not from 0.
    pub fn adopt_taps_from(&self, superseded: &ChainRuntimeState) {
        self.input_taps.store(superseded.input_taps.load_full());
        self.stream_taps.store(superseded.stream_taps.load_full());
        self.di_loop.store(superseded.di_loop.load_full());
        self.di_loop_pos.store(
            superseded.di_loop_pos.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        self.adopt_loopers_from(superseded);
    }

    /// #323: carry the recorded loops across an off-thread rebuild. The
    /// layers are plain buffers owned by the processing state, so the two
    /// banks are swapped wholesale — the rebuilt runtime keeps playing from
    /// the same position while the superseded one is dropped with the empty
    /// bank.
    ///
    /// A rebuild that changed the sample rate is the one case where the loops
    /// are dropped instead: the recorded frames belong to the old rate, and
    /// replaying them at the new one would play the loop at the wrong speed
    /// (the #669 bug). Buffer sizes differ in that case, which is exactly
    /// what the guard tests.
    fn adopt_loopers_from(&self, superseded: &ChainRuntimeState) {
        if self.loopers.max_frames() != superseded.loopers.max_frames() {
            return;
        }
        self.loopers.adopt_status_from(&superseded.loopers);
        if let (Ok(mut fresh), Ok(mut old)) = (self.processing.lock(), superseded.processing.lock())
        {
            std::mem::swap(&mut fresh.looper_bank, &mut old.looper_bank);
        }
    }

    /// Drop stream taps whose consumer handles have all been released.
    /// Mirrors `prune_dead_input_taps`.
    pub fn prune_dead_stream_taps(&self) {
        let current = self.stream_taps.load_full();
        let mut kept: Vec<Arc<StreamTap>> = Vec::with_capacity(current.len());
        let mut changed = false;
        for tap in current.iter() {
            // A consumer holds an Arc to either or both rings; if neither
            // ring has any external Arc, this tap is dead.
            let has_consumer =
                Arc::strong_count(&tap.l_ring) > 1 || Arc::strong_count(&tap.r_ring) > 1;
            if has_consumer {
                kept.push(Arc::clone(tap));
            } else {
                changed = true;
            }
        }
        if changed {
            self.stream_taps.store(Arc::new(kept));
        }
    }
}
