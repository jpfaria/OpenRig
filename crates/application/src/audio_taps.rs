//! #127: the SUBSCRIPTION seam — the third door, alongside
//! [`crate::runtime_control::RuntimeControl`] (writes) and
//! [`crate::live_source::LiveSource`] (reads).
//!
//! Those two carry finished values in one direction. A tap is neither: it is a
//! standing subscription that the frontend opens once and then polls on its own
//! tick. `LiveSource` returns readings and a `Command` cannot return a ring, so
//! before this the analyzers and the meters reached
//! `infra_cpal::ProjectRuntimeController` directly — and the narrowing that did
//! exist (`meter_wiring::MeterTapApi`) was declared in the GUI with a blanket
//! impl on the backend, i.e. the UI defining the engine's shape for itself.
//!
//! **Identity, never grouping (`CLAUDE.md` LAW).** A [`TapPoint`] names a chain
//! and a stream/input index. There is no way to express "every runtime at this
//! rate" or "all that match" — the failure mode the LAW is about is not
//! representable in the type.
//!
//! **Reduced by default, raw by exception.** [`AudioTap::poll_peak_dbfs`] is the
//! primary method: a finished number, the only shape a remote transport can
//! carry, and what the meters use. [`AudioTap::drain_channel`] hands out the raw
//! window and **defaults to giving nothing**, because an implementation that
//! does not sit in the same process as the audio has no samples to give. A
//! frontend that implements only the reduced method is correct, not broken.
//!
//! **What a remote frontend does instead of asking for samples: it does not.**
//! The consumers that need raw windows (tuner, spectrum, Tone Doctor) run their
//! analysis next to the audio and publish the RESULT — `LiveSource::tuner` /
//! `LiveSource::spectrum` and the `DiagnoseChainTone` command — so a remote
//! client is served finished readings and never needs the PCM to travel.
//!
//! **Not an audio-thread interface.** Subscribing and polling both happen on the
//! frontend's own thread. An implementation must not push allocation, locks or
//! I/O onto the audio callback: the local one registers the very same taps the
//! engine already had, so the audio thread's work is unchanged by construction.

use std::sync::Arc;

use domain::ids::ChainId;

/// WHICH tap, by the identity of the stream that produces it.
///
/// Every variant carries a [`ChainId`] and the index of the stream (or input)
/// within that chain. Two chains, or two streams of one chain, are two
/// different tap points that share nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TapPoint {
    /// Pre-FX device input of ONE stream of ONE chain, on the first device
    /// channel that stream's endpoint is wired to (#557). What a level meter
    /// reads.
    StreamInput { chain: ChainId, stream: usize },
    /// Post-FX, pre-mixdown stereo of ONE stream of ONE chain. Always two
    /// channels: a stream is internally stereo (invariant #5).
    StreamOutput { chain: ChainId, stream: usize },
    /// Every device channel of ONE input of ONE chain, as one multi-channel
    /// tap. The tuner's shape: one subscription, one channel per row.
    ///
    /// Subscribing per channel instead would register N taps where the engine
    /// needs one, which is extra work on the audio callback — so this stays a
    /// single tap and the consumer addresses its channels by index.
    InputChannels {
        chain: ChainId,
        input: usize,
        /// Device-side channel count of the endpoint; at least
        /// `max(channels) + 1`.
        total_channels: usize,
        /// The device channels this chain's input is actually wired to, in
        /// order. Channel `i` of the returned tap is `channels[i]`.
        channels: Vec<usize>,
    },
}

/// A live subscription the frontend holds open and polls on its own tick.
///
/// `Send + Sync` because a subscription may be handed to a worker — the Tone
/// Doctor prepares the capture on the dispatching thread and hands the blocking
/// fill over as a `Send` closure. The *authority* that issues subscriptions
/// ([`AudioTaps`]) is deliberately not `Send`: it is frontend-local.
///
/// A tap is SINGLE-CONSUMER. Both poll methods CONSUME the window they report,
/// so two readers of one tap would each see half the signal.
pub trait AudioTap: Send + Sync {
    /// How many channels this tap carries.
    fn channels(&self) -> usize;

    /// Peak dBFS over everything that arrived since the previous poll, across
    /// every channel of THIS tap, or [`engine::output_meter::SILENT_DBFS`] when
    /// nothing arrived.
    ///
    /// The reduced reading — implementable over any transport, and what the
    /// meters use.
    fn poll_peak_dbfs(&self) -> f32;

    /// Append at most `max` samples of channel `channel` to `out`, returning
    /// how many were appended.
    ///
    /// **Defaults to none.** Raw samples are an in-process affordance: an
    /// implementation that is not in the same address space as the audio has
    /// none to give, and a consumer that gets none simply shows no reading
    /// (see the module docs for what a remote frontend does instead).
    fn drain_channel(&self, channel: usize, max: usize, out: &mut Vec<f32>) -> usize {
        let _ = (channel, max, out);
        0
    }
}

/// The authority that issues subscriptions — the frontend that hosts the audio.
///
/// Every method has a "hosts nothing" default, so a transport with no audio
/// runtime implements nothing and its consumers degrade to silence instead of
/// failing.
pub trait AudioTaps {
    /// Is there a live runtime to subscribe against at all.
    ///
    /// Distinct from "this chain has no streams": `false` means the whole
    /// runtime is gone (the rig was stopped), which is what invalidates every
    /// handle a consumer is holding.
    fn is_hosted(&self) -> bool {
        false
    }

    /// The rate the live streams are actually running at, or 0 when nothing is
    /// hosted. Used only as the last fallback for an input with no saved
    /// per-device rate (#723) — never as a substitute for resolving a device.
    fn live_sample_rate(&self) -> u32 {
        0
    }

    /// How many streams (independent input pipelines) this chain runs right
    /// now — the valid range of `stream` in a [`TapPoint`]. 0 when the chain
    /// has no runtime.
    fn stream_count(&self, chain: &ChainId) -> usize {
        let _ = chain;
        0
    }

    /// Open a subscription on `point`, sized `capacity_per_channel`.
    ///
    /// `None` ⇒ that stream is not there to tap (no runtime, index out of
    /// range). Never a tap that silently reads a different stream.
    fn subscribe(
        &self,
        point: &TapPoint,
        capacity_per_channel: usize,
    ) -> Option<Arc<dyn AudioTap>> {
        let _ = (point, capacity_per_channel);
        None
    }

    /// Release tap slots whose consumer handle is gone.
    ///
    /// Refcount-based on the implementation side, so this can never take a tap
    /// away from a live consumer: a consumer holding its `Arc` keeps its slot.
    /// Called after an invalidation, when the previous handles were dropped.
    fn prune_dead_taps(&self) {}
}

/// The empty authority: a frontend that hosts no audio at all.
pub struct NoAudioTaps;

impl AudioTaps for NoAudioTaps {}

#[cfg(test)]
#[path = "audio_taps_tests.rs"]
mod tests;
