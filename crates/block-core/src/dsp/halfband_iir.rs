//! Responsibility: changes the sample rate by two through a polyphase IIR half-band filter.

use super::denormal::flush_denormal;
use super::halfband_design::{HalfbandCoefs, MAX_COEFS};

/// One branch of the polyphase pair: first-order allpasses in series,
/// `y[n] = c·(x[n] − y[n−1]) + x[n−1]`.
#[derive(Debug, Clone, Copy)]
struct AllpassChain {
    coefs: [f32; MAX_COEFS / 2],
    len: usize,
    x1: [f32; MAX_COEFS / 2],
    y1: [f32; MAX_COEFS / 2],
}

impl AllpassChain {
    /// The coefficients of `design` at even (`phase` 0) or odd (1) index.
    fn new(design: &HalfbandCoefs, phase: usize) -> Self {
        let mut coefs = [0.0; MAX_COEFS / 2];
        let mut len = 0;
        for &c in design.coefs[..design.len].iter().skip(phase).step_by(2) {
            coefs[len] = c;
            len += 1;
        }
        Self {
            coefs,
            len,
            x1: [0.0; MAX_COEFS / 2],
            y1: [0.0; MAX_COEFS / 2],
        }
    }

    #[inline]
    fn process(&mut self, mut x: f32) -> f32 {
        for i in 0..self.len {
            let y = flush_denormal(self.coefs[i] * (x - self.y1[i]) + self.x1[i]);
            self.x1[i] = x;
            self.y1[i] = y;
            x = y;
        }
        x
    }

    /// Delay at DC, in samples of the rate the chain runs at.
    fn dc_delay(&self) -> f32 {
        self.coefs[..self.len]
            .iter()
            .map(|c| (1.0 - c) / (1.0 + c))
            .sum()
    }
}

/// Doubles the rate: every input sample becomes two, band-limited to the
/// input's Nyquist.
#[derive(Debug, Clone, Copy)]
pub struct HalfbandUp {
    even: AllpassChain,
    odd: AllpassChain,
}

impl HalfbandUp {
    pub fn new(design: &HalfbandCoefs) -> Self {
        Self {
            even: AllpassChain::new(design, 0),
            odd: AllpassChain::new(design, 1),
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> [f32; 2] {
        [self.even.process(x), self.odd.process(x)]
    }

    /// Delay at DC, in samples of the doubled rate.
    pub fn dc_delay(&self) -> f32 {
        self.even.dc_delay() + self.odd.dc_delay() + 0.5
    }
}

/// Halves the rate: every two input samples become one, after removing
/// what would fold below the new Nyquist.
#[derive(Debug, Clone, Copy)]
pub struct HalfbandDown {
    even: AllpassChain,
    odd: AllpassChain,
}

impl HalfbandDown {
    pub fn new(design: &HalfbandCoefs) -> Self {
        Self {
            even: AllpassChain::new(design, 0),
            odd: AllpassChain::new(design, 1),
        }
    }

    #[inline]
    pub fn process(&mut self, pair: [f32; 2]) -> f32 {
        0.5 * (self.even.process(pair[1]) + self.odd.process(pair[0]))
    }

    /// Delay at DC, in samples of the doubled (input) rate.
    pub fn dc_delay(&self) -> f32 {
        self.even.dc_delay() + self.odd.dc_delay() + 0.5
    }
}
