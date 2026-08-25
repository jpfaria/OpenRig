//! Responsibility: cushions the DSP against output callback jitter.
//!
//! Split out of `runtime_audio_frame.rs` (#873).

use std::sync::atomic::{AtomicU64, Ordering};

use block_core::AudioChannelLayout;

use crate::audio_frame::{silent_frame, AudioFrame};
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
}

impl ElasticBuffer {
    pub(crate) fn new(target_level: usize, layout: AudioChannelLayout) -> Self {
        let init = silent_frame(layout);
        Self {
            ring: SpscRing::new(target_level.saturating_mul(2), init),
            target_level,
            layout,
            last_frame_bits: AtomicU64::new(frame_to_bits(init)),
            underrun_count: AtomicU64::new(0),
        }
    }

    /// Issue #670: number of underruns (empty `pop`s → silent gaps) since
    /// this buffer was built. Read off the audio thread.
    pub(crate) fn underrun_count(&self) -> u64 {
        self.underrun_count.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub(crate) fn push(&self, frame: AudioFrame) {
        self.last_frame_bits
            .store(frame_to_bits(frame), Ordering::Relaxed);
        // Drop-newest when full — the consumer is behind and a single dropped
        // sample is less disruptive than advancing the tail from the
        // producer side (which would violate the SPSC invariant).
        let _ = self.ring.push(frame);
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

    /// Pre-fill the buffer with `frames` silent frames so it starts at a
    /// real jitter cushion instead of empty. Used on the INITIAL build of a
    /// chain whose per-block worst-case latency (e.g. an IR convolver's
    /// per-partition FFT spike) can momentarily starve the consumer before
    /// the producer warms up — issue #592. The cushion costs `frames` of
    /// output latency; callers only prime when the chain warrants it.
    pub(crate) fn prime(&self, frames: usize) {
        let silence = silent_frame(self.layout);
        for _ in 0..frames {
            self.push(silence);
        }
    }

    /// Capacity target this buffer was built for (#670: rebuild route reuse).
    pub(crate) fn target_level(&self) -> usize {
        self.target_level
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
