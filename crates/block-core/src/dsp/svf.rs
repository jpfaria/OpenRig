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
#[path = "svf_tests.rs"]
mod tests;
