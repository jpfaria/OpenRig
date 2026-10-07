//! Responsibility: crossfades a chain's sound from the stream set an activation replaces to the set it opens.
//!
//! #1081: a structural edit (an IR swapped, a block added) and the
//! stepped-input restart open brand-new streams for the chain (#881, #979).
//! The old set used to be dropped the moment the new one was installed: its
//! output stopped on one sample while the new streams were still filling their
//! cushions and warming cold blocks — a click and a gap on every IR swap. Now
//! the new set plays unheard through a warm-up, then fades in while the old
//! set fades out with the complementary gain. Both sets are the SAME chain fed
//! the same input, so nothing crosses chains (invariant #4); the sum is the
//! backend's.
//!
//! Audio threads only read and raise atomics; the control side drops the old
//! set once the new one has taken over.

use std::f32::consts::PI;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

/// Output frames a new stream set plays unheard: its route cushions, the
/// runtime's fade from silence and its cold blocks (an IR's first partitions,
/// a fresh VST3's first buffers).
pub(crate) const STREAM_WARMUP_FRAMES: usize = 4096;
/// Output frames of the raised-cosine crossfade that follows.
pub(crate) const STREAM_FADE_FRAMES: usize = 2048;

/// One activation's stream set, as the next activation of its chain sees it.
pub(crate) struct StreamHandover {
    fades_in: bool,
    /// The most output frames any stream of the set has played.
    played: AtomicUsize,
    /// The set that replaces this one, once it is open.
    successor: OnceLock<Arc<StreamHandover>>,
}

impl StreamHandover {
    /// A set that replaces nothing: full level from its first frame.
    pub(crate) fn cold() -> Arc<Self> {
        Arc::new(Self::new(false))
    }

    /// A set that replaces live streams of its chain.
    pub(crate) fn replacing() -> Arc<Self> {
        Arc::new(Self::new(true))
    }

    fn new(fades_in: bool) -> Self {
        Self {
            fades_in,
            played: AtomicUsize::new(0),
            successor: OnceLock::new(),
        }
    }

    /// Control side: `next` replaces this set, which now fades out as `next`
    /// fades in.
    pub(crate) fn retire_into(&self, next: Arc<StreamHandover>) {
        let _ = self.successor.set(next);
    }

    /// `true` once the set plays at full level on its own.
    pub(crate) fn has_taken_over(&self) -> bool {
        !self.fades_in
            || self.played.load(Ordering::Acquire) >= STREAM_WARMUP_FRAMES + STREAM_FADE_FRAMES
    }

    /// The fade one output stream of the set applies to its callbacks.
    // cpal-only: the JACK path builds no output stream of its own.
    #[cfg_attr(
        all(target_os = "linux", feature = "jack", not(test)),
        allow(dead_code)
    )]
    pub(crate) fn output_fade(self: &Arc<Self>) -> OutputFade {
        OutputFade {
            set: Arc::clone(self),
            played: 0,
        }
    }
}

/// Gain of a set that fades in, `played` output frames into its life: silent
/// through the warm-up, then a raised cosine up to full level.
#[cfg_attr(
    all(target_os = "linux", feature = "jack", not(test)),
    allow(dead_code)
)]
fn incoming_gain(played: usize) -> f32 {
    let Some(into_fade) = played.checked_sub(STREAM_WARMUP_FRAMES) else {
        return 0.0;
    };
    if into_fade >= STREAM_FADE_FRAMES {
        return 1.0;
    }
    let progress = into_fade as f32 / STREAM_FADE_FRAMES as f32;
    0.5 * (1.0 - (PI * progress).cos())
}

/// One output stream's place in its set's handover; owned by its callback.
#[cfg_attr(
    all(target_os = "linux", feature = "jack", not(test)),
    allow(dead_code)
)]
pub(crate) struct OutputFade {
    set: Arc<StreamHandover>,
    played: usize,
}

#[cfg_attr(
    all(target_os = "linux", feature = "jack", not(test)),
    allow(dead_code)
)]
impl OutputFade {
    /// Audio side: scale one callback of interleaved `out` (`channels` wide)
    /// by the set's gain. No allocation, no lock.
    pub(crate) fn apply(&mut self, out: &mut [f32], channels: usize) {
        let channels = channels.max(1);
        let start = self.played;
        self.played = start.saturating_add(out.len() / channels);
        self.set.played.fetch_max(self.played, Ordering::AcqRel);
        let successor = self
            .set
            .successor
            .get()
            .map(|next| next.played.load(Ordering::Acquire));
        if !self.set.fades_in && successor.is_none() {
            return; // nothing to fade: the samples stay bit-identical
        }
        for (i, frame) in out.chunks_mut(channels).enumerate() {
            let own = if self.set.fades_in {
                incoming_gain(start + i)
            } else {
                1.0
            };
            let gain = own * successor.map_or(1.0, |next| 1.0 - incoming_gain(next + i));
            for sample in frame {
                *sample *= gain;
            }
        }
    }
}

#[cfg(test)]
#[path = "stream_handover_tests.rs"]
mod tests;
