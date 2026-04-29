//! Lock-free per-channel sample taps on chain inputs.
//!
//! Top-level OpenRig features (Tuner window, Spectrum window) need read-only
//! access to raw input samples without inserting a block into the chain.
//!
//! A consumer subscribes to one or more channels of a specific input on a
//! chain runtime and receives one [`SpscRing`] per channel. The audio thread
//! pushes samples into the rings inside `process_input_f32`; consumers poll
//! the rings on a UI/worker thread and run their own DSP (pitch detection,
//! FFT, etc.) without touching the RT path.
//!
//! ## Audio-thread invariants
//!
//! - The dispatch loop reads `runtime.input_taps` via `ArcSwap::load()`
//!   (lock-free) and pushes via `SpscRing::push` (lock-free). No allocation,
//!   no syscall, no block.
//! - When no subscribers are registered, the loaded `Vec` is empty and the
//!   dispatch returns immediately after the load. Cost is the ArcSwap load
//!   itself (~10–20 ns) — acceptable for a per-callback overhead.
//! - Rings drop incoming samples when full (consumer is too slow). This is
//!   safe: the consumer will read the next callback's samples instead. A
//!   detection round delay is preferable to blocking the audio thread.
//!
//! ## Lifetime
//!
//! Subscribers hold `Arc<SpscRing<f32>>` clones for as long as they want the
//! tap to keep producing samples. When the consumer drops the rings, the
//! reference count eventually reaches zero — the actual deallocation runs
//! on the thread that triggers it (typically the UI thread when the
//! consumer drops, since the audio thread only borrows for the duration of
//! one callback). Drop never runs on the audio thread in practice as long
//! as `runtime.input_taps` keeps at least one Arc alive.

use std::sync::Arc;

use crate::spsc::SpscRing;

/// A per-input subscription. One [`InputTap`] is created per consumer per
/// `input_index`; if the consumer wants multiple channels, they go in
/// `channel_rings`.
pub struct InputTap {
    /// Which input within the chain the tap targets — matches the
    /// `input_index` argument of `process_input_f32`.
    pub input_index: usize,
    /// One ring per channel of this input.
    ///
    /// Index in this `Vec` is the channel's position in the interleaved
    /// input data buffer (channel 0, 1, 2, ...). `None` means that channel
    /// is not subscribed and the audio thread skips it.
    pub channel_rings: Vec<Option<Arc<SpscRing<f32>>>>,
}

impl InputTap {
    /// Build a tap that subscribes to a specific set of channels of an
    /// input. `total_channels` is the input's full channel count (so the
    /// returned `channel_rings` has the right shape for index-based
    /// dispatch in the audio thread).
    ///
    /// `capacity_per_channel` is the SPSC ring capacity in samples. Pick a
    /// value that comfortably covers (consumer poll period) × (sample
    /// rate). For a tuner polling at 30 Hz @ 48 kHz, 4096 samples is
    /// already 2.5× the minimum.
    pub fn new(
        input_index: usize,
        total_channels: usize,
        subscribed_channels: &[usize],
        capacity_per_channel: usize,
    ) -> (Self, Vec<Arc<SpscRing<f32>>>) {
        let mut channel_rings: Vec<Option<Arc<SpscRing<f32>>>> =
            (0..total_channels).map(|_| None).collect();
        let mut consumer_handles: Vec<Arc<SpscRing<f32>>> =
            Vec::with_capacity(subscribed_channels.len());

        for &ch in subscribed_channels {
            if ch >= total_channels {
                continue;
            }
            let ring = Arc::new(SpscRing::<f32>::new(capacity_per_channel, 0.0));
            channel_rings[ch] = Some(Arc::clone(&ring));
            consumer_handles.push(ring);
        }

        (
            Self {
                input_index,
                channel_rings,
            },
            consumer_handles,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_returns_one_ring_per_subscribed_channel() {
        let (tap, handles) = InputTap::new(0, 4, &[0, 2], 256);
        assert_eq!(handles.len(), 2);
        // Tap stores Some only for subscribed channels
        assert!(tap.channel_rings[0].is_some());
        assert!(tap.channel_rings[1].is_none());
        assert!(tap.channel_rings[2].is_some());
        assert!(tap.channel_rings[3].is_none());
    }

    #[test]
    fn new_skips_out_of_range_channels() {
        let (tap, handles) = InputTap::new(0, 2, &[0, 5], 256);
        assert_eq!(handles.len(), 1);
        assert!(tap.channel_rings[0].is_some());
        assert!(tap.channel_rings[1].is_none());
    }

    #[test]
    fn rings_share_state_between_handles_and_tap() {
        let (tap, handles) = InputTap::new(0, 1, &[0], 256);
        let producer = tap.channel_rings[0].as_ref().unwrap();
        assert!(producer.push(1.5));
        // Consumer side reads through their handle
        let consumer = &handles[0];
        assert_eq!(Arc::as_ptr(producer), Arc::as_ptr(consumer));
    }
}
