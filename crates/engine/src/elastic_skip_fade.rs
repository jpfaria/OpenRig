//! Responsibility: crossfades across the frames an output route discards.
//!
//! #953: shedding stuck latency jumps the route forward in time. Cutting
//! straight across would click; this holds the first frames that would have
//! played and fades them into the frames that play instead.
//!
//! Lives on the output callback's stack: fixed array, `Copy` frames, no
//! allocation (invariant #8).

use crate::audio_frame::AudioFrame;

/// Length of the crossfade, in frames (~0.7 ms at 44.1 kHz).
pub(crate) const FADE_FRAMES: usize = 32;

pub(crate) struct SkipFade {
    old: [AudioFrame; FADE_FRAMES],
    len: usize,
}

impl SkipFade {
    pub(crate) fn none() -> Self {
        Self {
            old: [AudioFrame::Mono(0.0); FADE_FRAMES],
            len: 0,
        }
    }

    /// Keep one discarded frame to fade out of. Ignored once full.
    #[inline]
    pub(crate) fn hold(&mut self, frame: AudioFrame) {
        if self.len < FADE_FRAMES {
            self.old[self.len] = frame;
            self.len += 1;
        }
    }

    /// Frame `i` of the callback: the kept `frame`, faded in over the held
    /// frames while any remain.
    #[inline]
    pub(crate) fn blend(&self, i: usize, frame: AudioFrame) -> AudioFrame {
        if i >= self.len {
            return frame;
        }
        let a = (i + 1) as f32 / (self.len + 1) as f32;
        match (self.old[i], frame) {
            (AudioFrame::Stereo([ol, or]), AudioFrame::Stereo([l, r])) => {
                AudioFrame::Stereo([ol + (l - ol) * a, or + (r - or) * a])
            }
            (AudioFrame::Mono(o), AudioFrame::Mono(s)) => AudioFrame::Mono(o + (s - o) * a),
            _ => frame,
        }
    }
}

#[cfg(test)]
#[path = "elastic_skip_fade_tests.rs"]
mod tests;
