//! Responsibility: delays one split path so both paths meet in time.
//!
//! Spec §4.3: summing two paths with different latency comb-filters. The
//! shorter path is delayed by the difference in a ring preallocated at
//! build; the longer path is never delayed, so the chain keeps its latency.

use crate::runtime_audio_frame::AudioFrame;

/// Longest alignment a split preallocates: 16384 samples ≈ 341 ms at 48 kHz.
pub(crate) const MAX_ALIGN_SAMPLES: usize = 16_384;

/// How far each path is delayed, and whether the cap cut the difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AlignPlan {
    pub(crate) delay_a: usize,
    pub(crate) delay_b: usize,
    pub(crate) clamped: bool,
}

pub(crate) fn plan_alignment(latency_a: usize, latency_b: usize) -> AlignPlan {
    let difference = latency_a.abs_diff(latency_b);
    let applied = difference.min(MAX_ALIGN_SAMPLES);
    let (delay_a, delay_b) = if latency_b > latency_a {
        (applied, 0)
    } else {
        (0, applied)
    };
    AlignPlan {
        delay_a,
        delay_b,
        clamped: difference > MAX_ALIGN_SAMPLES,
    }
}

/// One path's delay line: a ring of stereo frames, sized at build.
pub(crate) struct AlignDelay {
    ring: Vec<[f32; 2]>,
    write: usize,
    delay: usize,
}

impl AlignDelay {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            ring: vec![[0.0; 2]; capacity.min(MAX_ALIGN_SAMPLES) + 1],
            write: 0,
            delay: 0,
        }
    }

    pub(crate) fn capacity(&self) -> usize {
        self.ring.len() - 1
    }

    #[cfg(test)]
    pub(crate) fn delay(&self) -> usize {
        self.delay
    }

    /// Delay the path by `delay` samples, never past the ring's capacity.
    /// No allocation: safe on the audio thread after a toggle.
    pub(crate) fn set_delay(&mut self, delay: usize) {
        self.delay = delay.min(self.capacity());
    }

    /// Continue from a previous build's ring (#328): a rebuild keeps what
    /// the delayed path already recorded instead of restarting it from
    /// silence. A ring of another size is not adopted.
    pub(crate) fn adopt_history(&mut self, previous: AlignDelay) {
        if previous.capacity() == self.capacity() {
            let delay = self.delay;
            *self = previous;
            self.delay = delay;
        }
    }

    /// Delay `frames` in place. The ring records every frame even at delay
    /// 0, so a later delay change reads real history.
    #[inline]
    pub(crate) fn process(&mut self, frames: &mut [AudioFrame]) {
        let len = self.ring.len();
        if len == 1 {
            return;
        }
        for frame in frames.iter_mut() {
            let input = match *frame {
                AudioFrame::Mono(sample) => [sample, sample],
                AudioFrame::Stereo(pair) => pair,
            };
            self.ring[self.write] = input;
            if self.delay > 0 {
                let read = (self.write + len - self.delay) % len;
                *frame = AudioFrame::Stereo(self.ring[read]);
            }
            self.write = (self.write + 1) % len;
        }
    }
}

#[cfg(test)]
#[path = "runtime_split_align_tests.rs"]
mod tests;
