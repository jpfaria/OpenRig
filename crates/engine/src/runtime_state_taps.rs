//! `ChainRuntimeState` tap subscription, pruning, migration, and per-stream
//! query methods (issue #792 split — single responsibility).
//!
//! The UI subscribes lock-free `SpscRing` taps (meter / spectrum / tuner) to a
//! chain's live audio; these methods manage that registry — subscribe, prune
//! released taps, migrate taps onto a rebuilt runtime, and expose the per-stream
//! routing the UI needs to subscribe correctly. Same `impl ChainRuntimeState`,
//! same public API — a pure move out of `runtime_state.rs`.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use domain::ids::BlockId;

use crate::input_tap::InputTap;
use crate::runtime_state::ChainRuntimeState;
use crate::spsc::SpscRing;
use crate::stream_tap::StreamTap;

impl ChainRuntimeState {
    /// Subscribe to raw pre-FX samples from one input. Returns one
    /// [`SpscRing`] handle per requested channel, in the same order as
    /// `subscribed_channels`. The audio thread starts pushing samples on
    /// the next callback. Drop the returned handles to unsubscribe — the
    /// tap is removed from the registry on the next subscription change.
    ///
    /// `total_channels` should match the input's actual interleaved channel
    /// count (i.e. the `input_total_channels` argument the audio callback
    /// receives). `capacity_per_channel` is the SPSC ring depth in samples;
    /// pick a value comfortably larger than (consumer poll period) ×
    /// (sample rate).
    pub fn subscribe_input_tap(
        &self,
        input_index: usize,
        total_channels: usize,
        subscribed_channels: &[usize],
        capacity_per_channel: usize,
    ) -> Vec<Arc<SpscRing<f32>>> {
        let (tap, handles) = InputTap::new(
            input_index,
            total_channels,
            subscribed_channels,
            capacity_per_channel,
        );
        let mut new_taps: Vec<Arc<InputTap>> =
            self.input_taps.load_full().iter().cloned().collect();
        new_taps.push(Arc::new(tap));
        self.input_taps.store(Arc::new(new_taps));
        handles
    }

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

    /// Subscribe a consumer to the post-FX, pre-mixdown stereo samples
    /// of stream `stream_index` (one input pipeline) in this chain.
    /// Returns `[l_ring, r_ring]` — both rings always present because
    /// every stream is internally stereo (mono inputs are upmixed
    /// before the FX chain).
    ///
    /// Same lock-free semantics as `subscribe_input_tap`. The dispatch
    /// happens before the device-side `output_muted` zero-fill so muting
    /// the audio output does not blank the analyzer.
    pub fn subscribe_stream_tap(
        &self,
        stream_index: usize,
        capacity_per_channel: usize,
    ) -> [Arc<SpscRing<f32>>; 2] {
        let (tap, handles) = StreamTap::new(stream_index, capacity_per_channel);
        let mut new_taps: Vec<Arc<StreamTap>> =
            self.stream_taps.load_full().iter().cloned().collect();
        new_taps.push(Arc::new(tap));
        self.stream_taps.store(Arc::new(new_taps));
        handles
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
        if let (Ok(mut fresh), Ok(mut old)) =
            (self.processing.lock(), superseded.processing.lock())
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

    /// How many streams this chain currently runs (one per
    /// `InputProcessingState`). Used by the UI to know how many
    /// per-stream taps to subscribe.
    ///
    /// Reads a lock-free atomic mirror updated by `runtime_graph` on
    /// build / rebuild (issue #580). Never blocks — the meter polling
    /// timer calls this at 30 Hz on the GUI thread, and any lock taken
    /// here contends with the audio thread's `try_lock` on
    /// `processing`, silencing callbacks at small buffer sizes.
    pub fn stream_count(&self) -> usize {
        self.stream_count.load(Ordering::Relaxed)
    }

    /// For a given LOCAL stream index in this runtime
    /// (`0..stream_count()`), return the input routing metadata needed
    /// to subscribe its per-stream INPUT meter tap:
    /// `(cpal_input_index, total_channels, device_channels)` where:
    ///
    /// - `cpal_input_index` is the runtime's cpal-callback group index
    ///   the tap must filter on (the value `process_input_f32` is
    ///   called with by the cpal stream that owns this segment).
    /// - `total_channels` is `max(device_channels) + 1`, sized so the
    ///   `InputTap`'s `channel_rings` Vec covers every subscribed
    ///   channel. Higher cpal-side counts are tolerated by the tap
    ///   dispatch loop (it skips channels past its own length).
    /// - `device_channels` is the set of interleaved-frame channels
    ///   this segment reads from (one entry for mono / split-mono /
    ///   single-channel mono, two for stereo / dual-mono). Issue #557:
    ///   the UI must subscribe THESE channels, not a default `[0]`,
    ///   so a chain wired to device channel 1 actually sees channel
    ///   1's signal in its meter ring.
    ///
    /// Returns `None` if `local_stream_index >= stream_count()` or no
    /// cpal group hosts that segment (degenerate, only happens during
    /// a teardown-rebuild race).
    pub fn input_routing_for_stream(
        &self,
        local_stream_index: usize,
    ) -> Option<(usize, usize, Vec<usize>)> {
        let processing = self.processing.lock().ok()?;
        let input_state = processing.input_states.get(local_stream_index)?;
        let cpal_input_index = processing
            .input_to_segments
            .iter()
            .position(|seg_idxs| seg_idxs.contains(&local_stream_index))?;
        let device_channels = input_state.input_channels.clone();
        let total_channels = device_channels
            .iter()
            .copied()
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        Some((cpal_input_index, total_channels, device_channels))
    }

    /// Returns stream data for a block by ID, or None if not found or empty.
    ///
    /// The inner read is wait-free (`ArcSwap::load`); only the outer
    /// `stream_handles` HashMap takes a brief lock against rebuild paths
    /// (also UI thread, never the RT callback).
    pub fn poll_stream(&self, block_id: &BlockId) -> Option<Vec<block_core::StreamEntry>> {
        let handles = self.stream_handles.lock().ok()?;
        let handle = handles.get(block_id)?;
        let entries = handle.load();
        if entries.is_empty() {
            None
        } else {
            Some((**entries).clone())
        }
    }
}
