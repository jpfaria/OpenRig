//! Lock-free per-stream sample taps.
//!
//! A "stream" is one [`InputProcessingState`] (one input feeding one
//! parallel pipeline through the chain). Each stream is internally stereo
//! — mono inputs are upmixed before the FX chain. The Spectrum window
//! subscribes one tap per stream and gets two `SpscRing<f32>` (L + R)
//! holding the post-FX, pre-mixdown signal for that stream.
//!
//! ## Why per-stream and not per-output-channel
//!
//! With multiple inputs (e.g. two guitars), the chain mixes every input's
//! processed signal into shared output routes. Tapping at the output
//! shows the *combined* spectrum — both guitars overlapping. The user
//! wants one analyzer per guitar, so we tap per-stream, before the mix.
//!
//! ## Audio-thread invariants
//!
//! - The dispatch loop reads `runtime.stream_taps` via `ArcSwap::load()`
//!   (lock-free) and pushes via `SpscRing::push` (lock-free). No
//!   allocation, no syscall, no block.
//! - When no subscribers are registered, the loaded `Vec` is empty and
//!   the dispatch returns immediately after the load. Cost is the
//!   ArcSwap load itself (~10–20 ns) — same as the input/output taps.
//! - Rings drop incoming samples when full. Safe back-pressure: the
//!   analyzer reads the next callback's samples instead of stalling.

use std::sync::Arc;

use crate::spsc::SpscRing;

/// A per-stream subscription. One `StreamTap` per consumer per
/// `stream_index`. Always carries two rings (L + R) because every stream
/// is stereo internally.
pub struct StreamTap {
    /// Which input-state index in the chain runtime this tap targets.
    /// Matches the `seg_idx` passed to `process_single_segment`.
    pub stream_index: usize,
    pub l_ring: Arc<SpscRing<f32>>,
    pub r_ring: Arc<SpscRing<f32>>,
}

impl StreamTap {
    /// Build a tap for a specific stream. `capacity_per_channel` is the
    /// SPSC ring depth in samples — pick at least `FFT_SIZE × 2` if the
    /// consumer is a spectrum analyzer.
    pub fn new(
        stream_index: usize,
        capacity_per_channel: usize,
    ) -> (Self, [Arc<SpscRing<f32>>; 2]) {
        let l = Arc::new(SpscRing::<f32>::new(capacity_per_channel, 0.0));
        let r = Arc::new(SpscRing::<f32>::new(capacity_per_channel, 0.0));
        (
            Self {
                stream_index,
                l_ring: Arc::clone(&l),
                r_ring: Arc::clone(&r),
            },
            [l, r],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_returns_two_rings() {
        let (_tap, handles) = StreamTap::new(0, 256);
        assert_eq!(handles.len(), 2);
    }

    #[test]
    fn rings_share_state_between_handles_and_tap() {
        let (tap, handles) = StreamTap::new(3, 256);
        assert!(tap.l_ring.push(0.5));
        assert!(tap.r_ring.push(-0.25));
        assert_eq!(Arc::as_ptr(&tap.l_ring), Arc::as_ptr(&handles[0]));
        assert_eq!(Arc::as_ptr(&tap.r_ring), Arc::as_ptr(&handles[1]));
    }

    #[test]
    fn stream_index_is_preserved() {
        let (tap, _) = StreamTap::new(7, 64);
        assert_eq!(tap.stream_index, 7);
    }
}
