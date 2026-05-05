//! Low-frequency oscillator (LFO) with band-limited waveforms.
//!
//! Sine LFO is naturally band-limited. Triangular and saw waveforms
//! have discontinuities (in value or slope) that, even at sub-Hz
//! rates, alias when used to *modulate* a parameter that touches the
//! audio thread (delay-line read position, filter cutoff). The
//! mitigation is **PolyBLEP** — Polynomial Band-Limited stEP — which
//! adds a small correction polynomial in a 1-sample window around
//! every wrap point, killing the harshest harmonics without an FIR
//! pre-filter.
//!
//! Reference: Välimäki & Huovilainen (2007), "Antialiasing Oscillators
//! in Subtractive Synthesis," IEEE Signal Processing Magazine,
//! pp. 116-125.
//!
//! RT-safe: pure scalar arithmetic, no allocation.

use std::f32::consts::TAU;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LfoShape {
    Sine,
    Triangle,
    Saw,
}

pub struct Lfo {
    shape: LfoShape,
    sample_rate: f32,
    rate_hz: f32,
    /// Phase in [0, 1).
    phase: f32,
    /// Phase increment per sample = rate_hz / sample_rate.
    inc: f32,
}

impl Lfo {
    pub fn new(shape: LfoShape, rate_hz: f32, sample_rate: f32) -> Self {
        Self {
            shape,
            sample_rate,
            rate_hz,
            phase: 0.0,
            inc: rate_hz / sample_rate,
        }
    }

    pub fn set_rate(&mut self, rate_hz: f32) {
        self.rate_hz = rate_hz;
        self.inc = rate_hz / self.sample_rate;
    }

    pub fn set_phase(&mut self, phase: f32) {
        self.phase = phase.rem_euclid(1.0);
    }

    pub fn rate_hz(&self) -> f32 {
        self.rate_hz
    }

    /// Returns the next sample in [-1, 1].
    pub fn next(&mut self) -> f32 {
        let p = self.phase;
        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        match self.shape {
            LfoShape::Sine => (TAU * p).sin(),
            LfoShape::Saw => {
                // Naive saw goes 2*p - 1; subtract PolyBLEP at wrap.
                let mut s = 2.0 * p - 1.0;
                s -= poly_blep(p, self.inc);
                s
            }
            LfoShape::Triangle => {
                // Generate triangle as integral of band-limited square
                // — the cheapest BL triangle: square BLEP-corrected
                // followed by leaky integration. We use the simpler
                // closed form directly: tri = 1 - |2p - 1| * 2 + 1
                // and BLAMP-correct the corners.
                let mut tri = 1.0 - (2.0 * p - 1.0).abs() * 2.0;
                // BLAMP at the two corners (p=0 and p=0.5).
                tri += poly_blamp(p, self.inc) * 4.0 * self.inc;
                let p_shift = (p + 0.5).rem_euclid(1.0);
                tri -= poly_blamp(p_shift, self.inc) * 4.0 * self.inc;
                tri
            }
        }
    }

    /// Returns the next sample mapped to [0, 1] (handy for a
    /// modulation envelope around 0.5).
    pub fn next_unipolar(&mut self) -> f32 {
        0.5 * (1.0 + self.next())
    }
}

/// PolyBLEP correction (Välimäki & Huovilainen 2007). `t` is the
/// current phase in [0,1), `dt` is the per-sample phase step.
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        2.0 * x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + 2.0 * x + 1.0
    } else {
        0.0
    }
}

/// PolyBLAMP — band-limited ramp, the integral of PolyBLEP. Used for
/// triangle / TRI corners which have a slope discontinuity (BLEP fixes
/// value discontinuities; BLAMP fixes slope discontinuities).
fn poly_blamp(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        let y = x - 1.0;
        -1.0 / 3.0 * y * y * y
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        let y = x + 1.0;
        1.0 / 3.0 * y * y * y
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_is_bounded() {
        let mut lfo = Lfo::new(LfoShape::Sine, 4.0, 44_100.0);
        for _ in 0..44_100 {
            let s = lfo.next();
            assert!(s.is_finite() && s.abs() <= 1.0 + 1e-6, "out of range: {s}");
        }
    }

    #[test]
    fn saw_is_bounded() {
        let mut lfo = Lfo::new(LfoShape::Saw, 4.0, 44_100.0);
        for _ in 0..44_100 {
            let s = lfo.next();
            assert!(s.is_finite() && s.abs() <= 1.0 + 0.5, "out of range: {s}");
        }
    }

    #[test]
    fn triangle_is_bounded() {
        let mut lfo = Lfo::new(LfoShape::Triangle, 4.0, 44_100.0);
        for _ in 0..44_100 {
            let s = lfo.next();
            assert!(s.is_finite() && s.abs() <= 1.0 + 0.1, "out of range: {s}");
        }
    }

    #[test]
    fn unipolar_in_zero_one() {
        let mut lfo = Lfo::new(LfoShape::Sine, 4.0, 44_100.0);
        for _ in 0..44_100 {
            let s = lfo.next_unipolar();
            assert!(s.is_finite() && s >= -1e-6 && s <= 1.0 + 1e-6);
        }
    }

    #[test]
    fn rate_change_takes_effect() {
        let mut lfo = Lfo::new(LfoShape::Sine, 1.0, 44_100.0);
        lfo.set_rate(10.0);
        assert!((lfo.rate_hz() - 10.0).abs() < 1e-9);
    }
}
