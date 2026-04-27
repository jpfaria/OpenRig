//! Lock-free per-channel sample taps on chain outputs.
//!
//! Symmetric to [`crate::input_tap::InputTap`], but the tap point is on the
//! output side — after every effect block has run, before the audio leaves
//! the engine. The Spectrum window subscribes one ring per channel of every
//! terminal output entry and runs its FFT off the RT path.
//!
//! ## Audio-thread invariants
//!
//! - The dispatch loop reads `runtime.output_taps` via `ArcSwap::load()`
//!   (lock-free) and pushes via `SpscRing::push` (lock-free). No allocation,
//!   no syscall, no block.
//! - When no subscribers are registered, the loaded `Vec` is empty and the
//!   dispatch returns immediately after the load. Cost is the ArcSwap load
//!   itself (~10–20 ns).
//! - Rings drop incoming samples when full (consumer is too slow). Safe:
//!   the analyzer will read the next callback's samples instead.
//! - Tap publish happens **before** any output-mute zero-fill, so the tuner's
//!   "Mute output" toggle does not silence the spectrum analyzer.
//!
//! ## Lifetime
//!
//! Same Drop semantics as `InputTap`: consumers hold `Arc<SpscRing<f32>>`
//! handles for as long as they want the tap to keep producing samples.
//! Periodic `prune_dead_output_taps()` cleans up taps whose handles have
//! been dropped.

use std::sync::Arc;

use crate::spsc::SpscRing;

/// A per-output subscription. One [`OutputTap`] is created per consumer per
/// `output_index`; if the consumer wants multiple channels, they go in
/// `channel_rings`.
pub struct OutputTap {
    /// Which output within the chain the tap targets — matches the
    /// `output_index` argument of `process_output_f32`.
    pub output_index: usize,
    /// One ring per channel of this output.
    ///
    /// Index in this `Vec` is the channel's position in the interleaved
    /// output data buffer (channel 0, 1, 2, ...). `None` means that channel
    /// is not subscribed and the audio thread skips it.
    pub channel_rings: Vec<Option<Arc<SpscRing<f32>>>>,
}

impl OutputTap {
    /// Build a tap that subscribes to a specific set of channels of an
    /// output. `total_channels` is the output's full channel count.
    ///
    /// `capacity_per_channel` is the SPSC ring capacity in samples. The
    /// spectrum analyzer needs at least `FFT_SIZE` samples to fill one
    /// frame; pick a value that comfortably covers `FFT_SIZE × poll_factor`
    /// (e.g. 2× FFT_SIZE = 16384 samples).
    pub fn new(
        output_index: usize,
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
                output_index,
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
        let (tap, handles) = OutputTap::new(0, 4, &[0, 2], 256);
        assert_eq!(handles.len(), 2);
        assert!(tap.channel_rings[0].is_some());
        assert!(tap.channel_rings[1].is_none());
        assert!(tap.channel_rings[2].is_some());
        assert!(tap.channel_rings[3].is_none());
    }

    #[test]
    fn new_skips_out_of_range_channels() {
        let (tap, handles) = OutputTap::new(0, 2, &[0, 5], 256);
        assert_eq!(handles.len(), 1);
        assert!(tap.channel_rings[0].is_some());
        assert!(tap.channel_rings[1].is_none());
    }

    #[test]
    fn rings_share_state_between_handles_and_tap() {
        let (tap, handles) = OutputTap::new(0, 1, &[0], 256);
        let producer = tap.channel_rings[0].as_ref().unwrap();
        assert!(producer.push(1.5));
        let consumer = &handles[0];
        assert_eq!(Arc::as_ptr(producer), Arc::as_ptr(consumer));
    }
}
