//! Low-latency time-domain pitch shifter (granular dual-tap overlap-add).
//!
//! Reusable foundation — `native_pitch_shifter` consumes it now, and Wave 2
//! of issue #381 (native autotune) reuses the same module. All buffers are
//! pre-allocated in `new()`, so `process_sample` is RT-safe (zero alloc,
//! zero lock, no FFT).
//!
//! ## Why time-domain, not a phase vocoder (issue #488)
//!
//! A phase vocoder's latency is ~one FFT window (≥40 ms for usable low-end
//! resolution on guitar) — unplayable live. It also collapsed every
//! partial's main lobe on the naive bin remap, warbling/cutting notes.
//! This is the method real guitar pitch pedals (DigiTech Whammy/Drop) use:
//! a circular delay line read by two taps half a grain out of phase, each
//! windowed by a Hann grain. The two Hann windows are offset by π so they
//! sum to exactly 1 — perfect amplitude reconstruction, no warble. No
//! pitch detection, so chords/polyphonic material survive (true PSOLA is
//! monophonic and would shatter on a chord). Latency ≈ half a grain.
//!
//! Pitch comes from the rate the read taps sweep the delay line:
//! `playback_rate = 1 − d(delay)/dt`. Setting the phase increment to
//! `(1 − pitch)/grain` makes `d(delay)/dt = 1 − pitch`, so the playback
//! rate is exactly `pitch` for any ratio.

pub const MIN_GRAIN: usize = 64;
pub const MAX_GRAIN: usize = 8192;
pub const MIN_PITCH_FACTOR: f32 = 0.25;
pub const MAX_PITCH_FACTOR: f32 = 4.0;

use std::f32::consts::TAU;

pub struct PitchEngine {
    buf: Vec<f32>,
    len: usize,
    write: usize,
    grain: f32,
    phase: f32,
    phase_inc: f32,
    pitch_factor: f32,
}

impl PitchEngine {
    /// `grain_len` sets the grain size in samples: latency ≈ `grain_len/2`,
    /// and the grain-rate warble is `|1−pitch|·sr/grain_len` Hz (bigger
    /// grain = less warble, more latency).
    pub fn new(grain_len: usize) -> Self {
        let grain_len = grain_len.clamp(MIN_GRAIN, MAX_GRAIN);
        // Need `grain` samples of history plus one for fractional interp,
        // plus headroom so the read tap never lands on the slot being
        // written this sample.
        let len = grain_len + 8;
        Self {
            buf: vec![0.0; len],
            len,
            write: 0,
            grain: grain_len as f32,
            phase: 0.0,
            phase_inc: 0.0, // pitch 1.0 until set
            pitch_factor: 1.0,
        }
    }

    pub fn set_pitch_factor(&mut self, factor: f32) {
        self.pitch_factor = factor.clamp(MIN_PITCH_FACTOR, MAX_PITCH_FACTOR);
        // d(delay)/dt = 1 − pitch ⇒ playback rate = pitch. The delay
        // sweeps one grain per `grain/|1−pitch|` samples.
        self.phase_inc = (1.0 - self.pitch_factor) / self.grain;
    }

    #[inline]
    fn read_interp(&self, delay: f32) -> f32 {
        // Read position `delay` samples behind the just-written sample.
        let pos = self.write as f32 - delay;
        let i = pos.floor();
        let frac = pos - i;
        let len = self.len as f32;
        // Wrap into [0, len) without a modulo on a possibly-negative int.
        let mut a = i % len;
        if a < 0.0 {
            a += len;
        }
        let i0 = a as usize;
        let i1 = if i0 + 1 == self.len { 0 } else { i0 + 1 };
        self.buf[i0] * (1.0 - frac) + self.buf[i1] * frac
    }

    pub fn process_sample(&mut self, input: f32) -> f32 {
        self.buf[self.write] = input;

        // Tap 2 is half a grain out of phase with tap 1.
        let p1 = self.phase;
        let mut p2 = self.phase + 0.5;
        if p2 >= 1.0 {
            p2 -= 1.0;
        }

        let s1 = self.read_interp(p1 * self.grain);
        let s2 = self.read_interp(p2 * self.grain);

        // Hann windows offset by π: (0.5−0.5cosθ) + (0.5−0.5cos(θ+π)) = 1
        // exactly, so the crossfade is amplitude-flat (no warble, the
        // #488 symptom).
        let w1 = 0.5 - 0.5 * (TAU * p1).cos();
        let w2 = 0.5 - 0.5 * (TAU * p2).cos();
        let y = w1 * s1 + w2 * s2;

        self.phase += self.phase_inc;
        self.phase -= self.phase.floor();
        self.write = if self.write + 1 == self.len {
            0
        } else {
            self.write + 1
        };
        y
    }
}

#[cfg(test)]
#[path = "pitch_engine_tests.rs"]
mod tests;
