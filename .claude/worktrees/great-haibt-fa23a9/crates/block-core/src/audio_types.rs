//! Audio-thread channel layout + per-model I/O contract.
//!
//! Lifted out of `lib.rs` (Phase 6 of issue #194). Pure type defs + a few
//! `const fn` predicates — no I/O, no DSP, no state.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

use arc_swap::ArcSwap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioChannelLayout {
    Mono,
    Stereo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelAudioMode {
    MonoOnly,
    DualMono,
    TrueStereo,
    MonoToStereo,
}

impl ModelAudioMode {
    pub const fn accepts_input(self, layout: AudioChannelLayout) -> bool {
        matches!(
            (self, layout),
            (Self::MonoOnly, AudioChannelLayout::Mono)
                | (Self::DualMono, AudioChannelLayout::Mono)
                | (Self::DualMono, AudioChannelLayout::Stereo)
                | (Self::TrueStereo, AudioChannelLayout::Stereo)
                | (Self::MonoToStereo, AudioChannelLayout::Mono)
                | (Self::MonoToStereo, AudioChannelLayout::Stereo)
        )
    }

    pub const fn output_layout(self, input: AudioChannelLayout) -> Option<AudioChannelLayout> {
        match self {
            Self::MonoOnly => match input {
                AudioChannelLayout::Mono => Some(AudioChannelLayout::Mono),
                AudioChannelLayout::Stereo => None,
            },
            Self::DualMono => Some(input),
            Self::TrueStereo => match input {
                AudioChannelLayout::Stereo => Some(AudioChannelLayout::Stereo),
                AudioChannelLayout::Mono => None,
            },
            Self::MonoToStereo => Some(AudioChannelLayout::Stereo),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MonoOnly => "mono_only",
            Self::DualMono => "dual_mono",
            Self::TrueStereo => "true_stereo",
            Self::MonoToStereo => "mono_to_stereo",
        }
    }
}

/// A single key-value entry in a real-time data stream.
/// Any block can publish stream entries for the GUI to display.
#[derive(Debug, Clone)]
pub struct StreamEntry {
    pub key: String,
    pub value: f32,
    pub text: String,
    /// Peak hold level (0.0–1.0). Used by spectrum-type streams; 0.0 for others.
    pub peak: f32,
}

/// Shared handle for publishing stream data from a processor to the GUI.
///
/// Wait-free on both sides: the producer (block worker thread) does
/// `stream.store(Arc::new(new_entries))` to publish a snapshot, and the
/// GUI does `stream.load()` to read the latest snapshot atomically. No
/// `Mutex`, no contention, no priority inversion. The producer's
/// `Arc::new(...)` allocation is acceptable because it runs on a worker
/// thread (e.g. `tuner-detection`, `spectrum-analyzer`) that the RT
/// audio callback only feeds via a bounded channel — never on the RT
/// callback path itself.
pub type StreamHandle = Arc<ArcSwap<Vec<StreamEntry>>>;

/// Construct a fresh, empty `StreamHandle`. Use this in block builders
/// instead of `Arc::new(Mutex::new(Vec::new()))`.
pub fn new_stream_handle() -> StreamHandle {
    Arc::new(ArcSwap::from_pointee(Vec::new()))
}
