//! Responsibility: resamples a route whose device runs at another rate.
//! Per-route sample-rate conversion for an output whose device does NOT run at
//! the chain's rate (issue #85).
//!
//! A mid `Output` can point at another interface, and interfaces disagree: the
//! owner's Scarlett runs at 44.1 kHz while the TEYUN he taps into cannot go
//! below 48 kHz. The chain produces frames on ITS clock; that device's callback
//! pops on its own. Without conversion the route's elastic buffer starves ~8 %
//! of the time — thousands of underruns with zero xruns, which is the crackle
//! he heard.
//!
//! Audio-thread contract: the state lives in the producer's callback scratch
//! (single producer per route), the history is a fixed array, and the only
//! allocation is the output `Vec`'s first growth — same as `mixed_per_route`.
//!
//! Catmull-Rom (4-point cubic) rather than linear: the extra cost is a handful
//! of multiplies per frame and it keeps the top octave clean, which linear
//! interpolation does not.

use crate::runtime_audio_frame::AudioFrame;

/// How hard the ratio follows the route's fill. Small on purpose: the loop has
/// to absorb a crystal's drift (tens of ppm), not chase per-callback jitter.
const TRACK_GAIN: f64 = 0.02;
/// Hard limit on the correction, so the loop can never bend the pitch audibly.
const MAX_TRIM: f64 = 0.002;

/// Converts one route's frame stream from the chain's rate to the device's.
pub(crate) struct RouteResampler {
    /// Nominal source frames per output frame (`in_rate / out_rate`).
    base_step: f64,
    /// The ratio actually in use — `base_step` trimmed by the tracking loop.
    step: f64,
    /// Smoothed fill of the route, in frames.
    fill: f64,
    /// Position of the next output frame between `hist[1]` and `hist[2]`.
    phase: f64,
    /// Four consecutive source frames: the cubic reads `[-1, 0, +1, +2]`.
    /// Seeded with silence, so the very first callback already produces its
    /// full share of frames — holding output back to prime the history left
    /// the route three frames short and it underran on the first pops.
    hist: [AudioFrame; 4],
}

#[inline]
fn scale(frame: AudioFrame, gain: f32) -> AudioFrame {
    match frame {
        AudioFrame::Mono(s) => AudioFrame::Mono(s * gain),
        AudioFrame::Stereo([l, r]) => AudioFrame::Stereo([l * gain, r * gain]),
    }
}

#[inline]
fn add(a: AudioFrame, b: AudioFrame) -> AudioFrame {
    match (a, b) {
        (AudioFrame::Mono(x), AudioFrame::Mono(y)) => AudioFrame::Mono(x + y),
        (AudioFrame::Stereo([l1, r1]), AudioFrame::Stereo([l2, r2])) => {
            AudioFrame::Stereo([l1 + l2, r1 + r2])
        }
        (AudioFrame::Stereo([l, r]), AudioFrame::Mono(m)) => AudioFrame::Stereo([l + m, r + m]),
        (AudioFrame::Mono(m), AudioFrame::Stereo([l, r])) => AudioFrame::Stereo([m + l, m + r]),
    }
}

/// Catmull-Rom between `h[1]` and `h[2]` at `t ∈ [0, 1)`.
#[inline]
fn interpolate(h: &[AudioFrame; 4], t: f32) -> AudioFrame {
    let t2 = t * t;
    let t3 = t2 * t;
    let c0 = -0.5 * t3 + t2 - 0.5 * t;
    let c1 = 1.5 * t3 - 2.5 * t2 + 1.0;
    let c2 = -1.5 * t3 + 2.0 * t2 + 0.5 * t;
    let c3 = 0.5 * t3 - 0.5 * t2;
    add(
        add(scale(h[0], c0), scale(h[1], c1)),
        add(scale(h[2], c2), scale(h[3], c3)),
    )
}

impl RouteResampler {
    /// `None` when the rates match (or either is unusable) — the caller then
    /// pushes the frames untouched, so a same-rate rig is bit-identical.
    pub(crate) fn new(in_rate: f32, out_rate: f32, layout_seed: AudioFrame) -> Option<Self> {
        if !(in_rate.is_finite() && out_rate.is_finite()) || in_rate <= 0.0 || out_rate <= 0.0 {
            return None;
        }
        if (in_rate - out_rate).abs() < f32::EPSILON {
            return None;
        }
        let step = in_rate as f64 / out_rate as f64;
        Some(Self {
            base_step: step,
            step,
            fill: 0.0,
            phase: 0.0,
            hist: [layout_seed; 4],
        })
    }

    /// Follow the route's real clock.
    ///
    /// Two interfaces have two crystals: a device nominally at 48 kHz runs a
    /// hair fast or slow against the chain's 44.1 kHz, and a FIXED ratio bleeds
    /// the route dry (or overflows it) at exactly that pace — the owner's 256
    /// underruns per poll with zero xruns. The ratio is trimmed by how far the
    /// route's cushion sits from its target: too empty ⇒ produce a little more,
    /// too full ⇒ a little less. Clamped to ±0.2 %, so the correction stays
    /// inaudible while covering any real crystal.
    #[inline]
    pub(crate) fn track(&mut self, fill: usize, target: usize) {
        if target == 0 {
            return;
        }
        // One-pole smoothing: the raw fill swings by a whole callback.
        self.fill += 0.05 * (fill as f64 - self.fill);
        let error = (self.fill - target as f64) / target as f64;
        let trim = (TRACK_GAIN * error).clamp(-MAX_TRIM, MAX_TRIM);
        self.step = self.base_step * (1.0 + trim);
    }

    /// Feed one source frame; append every output frame it completes.
    #[inline]
    pub(crate) fn push(&mut self, frame: AudioFrame, out: &mut Vec<AudioFrame>) {
        self.hist[0] = self.hist[1];
        self.hist[1] = self.hist[2];
        self.hist[2] = self.hist[3];
        self.hist[3] = frame;
        while self.phase < 1.0 {
            out.push(interpolate(&self.hist, self.phase as f32));
            self.phase += self.step;
        }
        self.phase -= 1.0;
    }
}

#[cfg(test)]
#[path = "runtime_route_resample_tests.rs"]
mod tests;
