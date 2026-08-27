//! Responsibility: answers what a chain's streams currently carry.
//!
//! Split out of `runtime_state_taps.rs` (#873).

use crate::runtime_state::ChainRuntimeState;
use domain::ids::BlockId;
use std::sync::atomic::Ordering;

impl ChainRuntimeState {
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
