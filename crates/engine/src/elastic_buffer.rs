//! Responsibility: cushions the DSP against output callback jitter.
//!
//! Split out of `runtime_audio_frame.rs` (#873).

use std::sync::atomic::{AtomicU64, Ordering};

use block_core::AudioChannelLayout;

use crate::audio_frame::{silent_frame, AudioFrame};
use crate::elastic_drift_guard::DriftGuard;
use crate::elastic_skip_fade::{SkipFade, FADE_FRAMES};
use crate::spsc::SpscRing;

/// Floor for the elastic buffer target. Below this the buffer cannot absorb
/// even minor scheduling jitter, regardless of how small the device buffer is.
pub const ELASTIC_TARGET_FLOOR: usize = 64;

/// Default elastic target used when no device-derived value is provided
/// (tests, headless tools). Production callers in infra-cpal compute this
/// from the resolved device buffer size via [`elastic_target_for_buffer`].
pub const DEFAULT_ELASTIC_TARGET: usize = 256;

/// Compute the elastic buffer target level (in frames) for a given device
/// buffer size and backend multiplier.
///
/// The elastic buffer absorbs jitter between the producer (input + DSP path)
/// and the consumer (output callback). Sizing it relative to the actual device
/// buffer makes the latency proportional to the user's chosen buffer size
/// instead of a hardcoded constant.
///
/// `multiplier` reflects backend-specific jitter:
/// - `2` — direct CPAL callbacks (macOS/Windows/Linux ALSA): tight, predictable.
/// - `8` — JACK with worker-thread DSP (Linux): non-RT worker adds variance.
pub fn elastic_target_for_buffer(buffer_size_frames: u32, multiplier: u8) -> usize {
    let target = (buffer_size_frames as usize).saturating_mul(multiplier as usize);
    target.max(ELASTIC_TARGET_FLOOR)
}

/// Elastic audio buffer for clock drift compensation.
///
/// Lock-free single-producer / single-consumer. The producer is the input
/// DSP path (`process_input_f32`); the consumer is the output callback
/// (`process_output_f32`). Both call `push`/`pop` with `&self`, so there is
/// no `Mutex` in the RT audio path.
///
/// On underrun `pop` returns the most recently pushed frame, providing a
/// brief sustain instead of silence.
pub(crate) struct ElasticBuffer {
    ring: SpscRing<AudioFrame>,
    target_level: usize,
    /// Frames the ring was asked to hold (#965): at least twice the target,
    /// more when a cold-start prime bigger than the target has to fit.
    capacity: usize,
    layout: AudioChannelLayout,
    /// Bit-packed last-pushed frame, used as the underrun fallback.
    /// Mono: `f32` bits in the low 32 bits.
    /// Stereo: left in low 32 bits, right in high 32 bits.
    last_frame_bits: AtomicU64,
    /// Issue #670 instrumentation: count of `pop`s that found the ring empty
    /// (underrun → a silent frame was emitted = an audible gap). Incremented
    /// on the output callback (RT-safe relaxed atomic, only on the rare
    /// empty branch). Read off-thread to tell an elastic-buffer underrun
    /// apart from a CPU deadline overrun (xrun): a single light chain at
    /// buffer 64 crackling with near-zero xruns points here, not at CPU.
    underrun_count: AtomicU64,
    /// #980: frames `push` discarded because the ring was full — a producer
    /// that ran late and caught up loses exactly these. Without this count a
    /// late producer and a producer that never pushed look the same.
    dropped_count: AtomicU64,
    /// #953: sheds latency a stalled output stream left in the ring.
    drift: DriftGuard,
    /// #965: off on a route whose level the #85 resampler servo holds.
    drift_guarded: bool,
}

impl ElasticBuffer {
    #[cfg(test)]
    pub(crate) fn new(target_level: usize, layout: AudioChannelLayout) -> Self {
        Self::with_capacity(target_level, target_level.saturating_mul(2), layout)
    }

    /// #965: a ring that rests at `target_level` but can hold `capacity`
    /// frames — room for a cold-start prime deeper than the resting level,
    /// which the drift guard sheds once the route runs clean.
    pub(crate) fn with_capacity(
        target_level: usize,
        capacity: usize,
        layout: AudioChannelLayout,
    ) -> Self {
        let init = silent_frame(layout);
        let capacity = capacity.max(target_level.saturating_mul(2));
        Self {
            ring: SpscRing::new(capacity, init),
            target_level,
            capacity,
            layout,
            last_frame_bits: AtomicU64::new(frame_to_bits(init)),
            underrun_count: AtomicU64::new(0),
            dropped_count: AtomicU64::new(0),
            drift: DriftGuard::new(target_level),
            drift_guarded: true,
        }
    }

    /// #965: hand the route's level to the #85 resampler servo — the drift
    /// guard no longer trims it (the two fought, refill and cut, forever).
    pub(crate) fn owned_by_servo(mut self) -> Self {
        self.drift_guarded = false;
        self
    }

    /// Issue #670: number of underruns (empty `pop`s → silent gaps) since
    /// this buffer was built. Read off the audio thread.
    pub(crate) fn underrun_count(&self) -> u64 {
        self.underrun_count.load(Ordering::Relaxed)
    }

    /// #980: frames discarded on a full ring since this buffer was built.
    /// Read off the audio thread.
    pub(crate) fn dropped_count(&self) -> u64 {
        self.dropped_count.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub(crate) fn push(&self, frame: AudioFrame) {
        self.last_frame_bits
            .store(frame_to_bits(frame), Ordering::Relaxed);
        // Drop-newest when full — the consumer is behind and a single dropped
        // sample is less disruptive than advancing the tail from the
        // producer side (which would violate the SPSC invariant). #980:
        // counted (one relaxed add, only on the full branch).
        if !self.ring.push(frame) {
            self.dropped_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[inline(always)]
    pub(crate) fn pop(&self) -> AudioFrame {
        // Issue #496: the previous form returned `last_frame_bits` on
        // underrun ("brief sustain instead of silence"). Measured cost:
        // every underrun produced a flat-top plateau / DC chunk in the
        // middle of the signal, injecting broadband harmonic distortion
        // and noise (the reported swarm-of-bees artefact). Silence is
        // the standard DAW behavior: a tiny gap is musically inaudible,
        // repeated samples are not.
        match self.ring.pop() {
            Some(frame) => frame,
            None => {
                // Issue #670: underrun — the producer (input DSP) hasn't
                // delivered this frame yet. Count it; the gap is the click.
                self.underrun_count.fetch_add(1, Ordering::Relaxed);
                silent_frame(self.layout)
            }
        }
    }

    /// #953: consumer side, once per output callback before its `frames`
    /// pops. Discards latency a stall left stuck in the ring and returns the
    /// crossfade the callback applies to its first pops.
    #[inline]
    pub(crate) fn begin_callback(&self, frames: usize) -> SkipFade {
        let mut fade = SkipFade::none();
        if !self.drift_guarded {
            return fade;
        }
        let skip = self
            .drift
            .observe(self.ring.len(), frames, self.underrun_count());
        for n in 0..skip {
            let Some(frame) = self.ring.pop() else {
                break;
            };
            if n < FADE_FRAMES {
                fade.hold(frame);
            }
        }
        fade
    }

    /// #953: times this route shed stuck latency since it was built.
    pub(crate) fn latency_trims(&self) -> u64 {
        self.drift.trims()
    }

    /// Pre-fill the buffer with `frames` silent frames so it starts at a
    /// real jitter cushion instead of empty. Used on the INITIAL build of a
    /// chain whose per-block worst-case latency (e.g. an IR convolver's
    /// per-partition FFT spike) can momentarily starve the consumer before
    /// the producer warms up — issue #592. The cushion costs `frames` of
    /// output latency; callers only prime when the chain warrants it.
    pub(crate) fn prime(&self, frames: usize) {
        self.drift.hold_rest(frames);
        let silence = silent_frame(self.layout);
        for _ in 0..frames {
            self.push(silence);
        }
    }

    /// Resting level this buffer was built for (#670: rebuild route reuse).
    pub(crate) fn target_level(&self) -> usize {
        self.target_level
    }

    /// Frames this buffer was built to hold (#965: rebuild route reuse — a
    /// route whose cushion posture changed is rebuilt, and primed, fresh).
    pub(crate) fn capacity(&self) -> usize {
        self.capacity
    }

    /// Channel layout this buffer was built for (#670: rebuild route reuse).
    pub(crate) fn layout(&self) -> AudioChannelLayout {
        self.layout
    }

    /// Seed the underrun fallback from another buffer's last pushed frame.
    /// Used during chain rebuild so that a brief underrun on the new buffer
    /// repeats the tail of the old buffer instead of jumping to silence.
    pub(crate) fn seed_last_frame_from(&self, other: &ElasticBuffer) {
        self.last_frame_bits.store(
            other.last_frame_bits.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
    }

    /// Frames currently queued. Read on the producer side by the per-route
    /// resampler's clock tracking (#85) and by tests.
    pub(crate) fn len(&self) -> usize {
        self.ring.len()
    }
}

#[inline(always)]
fn frame_to_bits(frame: AudioFrame) -> u64 {
    match frame {
        AudioFrame::Mono(s) => s.to_bits() as u64,
        AudioFrame::Stereo([l, r]) => (l.to_bits() as u64) | ((r.to_bits() as u64) << 32),
    }
}

#[inline(always)]
#[allow(dead_code)] // unused after issue #496: pop() returns silence on
                    // underrun, not the bit-packed last frame. Kept for
                    // potential future use (smooth fade-out fallback).
fn bits_to_frame(bits: u64, layout: AudioChannelLayout) -> AudioFrame {
    match layout {
        AudioChannelLayout::Mono => AudioFrame::Mono(f32::from_bits(bits as u32)),
        AudioChannelLayout::Stereo => {
            let l = f32::from_bits(bits as u32);
            let r = f32::from_bits((bits >> 32) as u32);
            AudioFrame::Stereo([l, r])
        }
    }
}
