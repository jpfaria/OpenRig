//! Responsibility: subscribes a reader to the samples flowing through a chain.
//!
//! Split out of `runtime_state_taps.rs` (#873).

use crate::input_tap::InputTap;
use crate::runtime_state::ChainRuntimeState;
use crate::spsc::SpscRing;
use crate::stream_tap::StreamTap;
use std::sync::Arc;

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
}
