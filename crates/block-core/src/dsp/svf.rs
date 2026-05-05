//! State-Variable Filter (Zero-Delay Feedback topology).
//!
//! Reference: Simper, A. (2011). "Linear Trapezoidal Integrated State
//! Variable Filter." Cytomic technical paper. ZDF SVF preserves the
//! analog-prototype behaviour at high resonance and high cutoff
//! sweep rates without the warping artefacts of a bilinear-substituted
//! biquad — perfect for wah, auto-wah, and any sweep-driven filter.
//!
//! Outputs all three responses (low-pass, band-pass, high-pass) from a
//! single shared state — saves CPU when a plugin needs a multi-mode
//! filter (auto-wah, multiband splitter, formant cascade).
//!
//! RT-safe: 2 state floats, ~12 ops per sample, no allocation.

use std::f32::consts::PI;

#[derive(Debug, Clone, Copy)]
pub struct SvfFrame {
    pub low: f32,
    pub band: f32,
    pub high: f32,
}

pub struct Svf {
    sample_rate: f32,
    g: f32,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    ic1eq: f32,
    ic2eq: f32,
}

impl Svf {
    pub fn new(cutoff_hz: f32, q: f32, sample_rate: f32) -> Self {
        let mut svf = Self {
            sample_rate,
            g: 0.0,
            k: 0.0,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            ic1eq: 0.0,
            ic2eq: 0.0,
        };
        svf.set_cutoff_q(cutoff_hz, q);
        svf
    }

    /// Update cutoff and Q. Costs one tan() — fine to call per sample
    /// for sweep-driven filters; if the modulator changes slowly,
    /// caller may amortise across N samples.
    #[inline]
    pub fn set_cutoff_q(&mut self, cutoff_hz: f32, q: f32) {
        let cutoff = cutoff_hz.clamp(10.0, self.sample_rate * 0.45);
        let q = q.max(0.05);
        self.g = (PI * cutoff / self.sample_rate).tan();
        self.k = 1.0 / q;
        let denom = 1.0 + self.g * (self.g + self.k);
        self.a1 = 1.0 / denom;
        self.a2 = self.g * self.a1;
        self.a3 = self.g * self.a2;
    }

    /// Process one sample, returning all three outputs.
    #[inline]
    pub fn process(&mut self, x: f32) -> SvfFrame {
        let v3 = x - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;

        let low = v2;
        let band = v1;
        let high = x - self.k * v1 - v2;
        SvfFrame { low, band, high }
    }

    /// Convenience: just the band-pass output.
    #[inline]
    pub fn process_band(&mut self, x: f32) -> f32 {
        self.process(x).band
    }

    /// Convenience: just the low-pass output.
    #[inline]
    pub fn process_low(&mut self, x: f32) -> f32 {
        self.process(x).low
    }

    /// Convenience: just the high-pass output.
    #[inline]
    pub fn process_high(&mut self, x: f32) -> f32 {
        self.process(x).high
    }

    pub fn reset(&mut self) {
        self.ic1eq = 0.0;
        self.ic2eq = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    #[test]
    fn silence_in_silence_out() {
        let mut svf = Svf::new(1_000.0, 2.0, 44_100.0);
        for _ in 0..2048 {
            let f = svf.process(0.0);
            assert_eq!(f.low, 0.0);
            assert_eq!(f.band, 0.0);
            assert_eq!(f.high, 0.0);
        }
    }

    #[test]
    fn sine_input_finite() {
        let mut svf = Svf::new(1_000.0, 2.0, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..4096 {
            let x = (TAU * 440.0 * i as f32 / sr).sin();
            let f = svf.process(x);
            assert!(f.low.is_finite() && f.band.is_finite() && f.high.is_finite());
        }
    }

    #[test]
    fn lowpass_attenuates_above_cutoff() {
        // Sine at 5 kHz through 200 Hz LP: low-pass output should be
        // much smaller than input.
        let mut svf = Svf::new(200.0, 0.7, 44_100.0);
        let sr = 44_100.0_f32;
        // Warm-up.
        for n in 0..8192 {
            let x = (TAU * 5_000.0 * n as f32 / sr).sin();
            svf.process(x);
        }
        let mut peak_in = 0.0_f32;
        let mut peak_out = 0.0_f32;
        for n in 8192..16_384 {
            let x = (TAU * 5_000.0 * n as f32 / sr).sin();
            let lp = svf.process_low(x);
            peak_in = peak_in.max(x.abs());
            peak_out = peak_out.max(lp.abs());
        }
        assert!(
            peak_out < 0.1 * peak_in,
            "LP didn't attenuate: out {peak_out} vs in {peak_in}"
        );
    }

    #[test]
    fn bandpass_resonates_at_cutoff() {
        // Sine at cutoff should have BP magnitude approximately ~1
        // (proportional to Q in this normalized form).
        let mut svf = Svf::new(1_000.0, 2.0, 44_100.0);
        let sr = 44_100.0_f32;
        // Warm-up.
        for n in 0..8192 {
            let x = (TAU * 1_000.0 * n as f32 / sr).sin();
            svf.process(x);
        }
        let mut peak = 0.0_f32;
        for n in 8192..16_384 {
            let x = (TAU * 1_000.0 * n as f32 / sr).sin();
            let bp = svf.process_band(x);
            peak = peak.max(bp.abs());
        }
        // Q=2 → BP peak gain ~Q/2 = 1.0 in this normalisation.
        assert!(peak > 0.5, "BP didn't resonate: peak {peak}");
    }

    #[test]
    fn coefficient_sweep_remains_stable() {
        // Sweep cutoff while feeding white-ish noise, ensure output stays bounded.
        let mut svf = Svf::new(500.0, 5.0, 44_100.0);
        let sr = 44_100.0_f32;
        for n in 0..44_100 {
            let cutoff = 100.0 + 4_000.0 * (0.5 + 0.5 * (TAU * 0.5 * n as f32 / sr).sin());
            svf.set_cutoff_q(cutoff, 5.0);
            let x = ((n as f32 * 17.0).sin()).clamp(-1.0, 1.0);
            let f = svf.process(x);
            assert!(f.band.abs() < 50.0, "diverged: {} at {n}", f.band);
        }
    }
}
