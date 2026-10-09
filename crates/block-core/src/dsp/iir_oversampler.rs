//! Responsibility: runs a mono stage at a power-of-two multiple of the stream rate.
//!
//! Each doubling is a polyphase IIR half-band pair (`halfband_iir`), so
//! the delay stays at a few samples where a linear-phase FIR oversampler
//! of the same rejection would add milliseconds. Every stage passes up to
//! 20 kHz (or 0.45 of the stream rate, if lower) and rejects 100 dB of
//! whatever would fold into it.

use super::halfband_design::design;
use super::halfband_iir::{HalfbandDown, HalfbandUp};
use crate::MonoProcessor;

/// Highest factor supported: 2^5 = 32.
const MAX_STAGES: usize = 5;
const MAX_FACTOR: usize = 1 << MAX_STAGES;
const STOPBAND_DB: f64 = 100.0;

pub struct IirOversampler {
    ups: [Option<HalfbandUp>; MAX_STAGES],
    downs: [Option<HalfbandDown>; MAX_STAGES],
    stages: usize,
    buf: [f32; MAX_FACTOR],
    tmp: [f32; MAX_FACTOR],
    latency: usize,
}

impl IirOversampler {
    /// `factor` is rounded up to a power of two and capped at 32.
    pub fn new(sample_rate: f32, factor: usize) -> Self {
        let stages = factor
            .clamp(1, MAX_FACTOR)
            .next_power_of_two()
            .trailing_zeros() as usize;
        let passband = 20_000.0_f64.min(0.45 * sample_rate as f64);
        let mut ups = [None; MAX_STAGES];
        let mut downs = [None; MAX_STAGES];
        let mut delay = 0.0_f32;
        let mut rate = sample_rate as f64;
        for s in 0..stages {
            let doubled = rate * 2.0;
            let coefs = design(STOPBAND_DB, 0.25 - passband / doubled);
            let up = HalfbandUp::new(&coefs);
            let down = HalfbandDown::new(&coefs);
            delay += (up.dc_delay() + down.dc_delay()) / (1 << (s + 1)) as f32;
            ups[s] = Some(up);
            downs[s] = Some(down);
            rate = doubled;
        }
        Self {
            ups,
            downs,
            stages,
            buf: [0.0; MAX_FACTOR],
            tmp: [0.0; MAX_FACTOR],
            latency: delay.round() as usize,
        }
    }

    pub fn factor(&self) -> usize {
        1 << self.stages
    }

    /// Run `f` on every oversampled sample of `x`; returns the band-limited
    /// result at the stream rate.
    #[inline]
    pub fn process(&mut self, x: f32, mut f: impl FnMut(f32) -> f32) -> f32 {
        self.buf[0] = x;
        let mut n = 1;
        for up in self.ups.iter_mut().take(self.stages).flatten() {
            for i in 0..n {
                let [a, b] = up.process(self.buf[i]);
                self.tmp[2 * i] = a;
                self.tmp[2 * i + 1] = b;
            }
            n *= 2;
            self.buf[..n].copy_from_slice(&self.tmp[..n]);
        }
        for s in self.buf[..n].iter_mut() {
            *s = f(*s);
        }
        for down in self.downs.iter_mut().take(self.stages).rev().flatten() {
            n /= 2;
            for i in 0..n {
                self.tmp[i] = down.process([self.buf[2 * i], self.buf[2 * i + 1]]);
            }
            self.buf[..n].copy_from_slice(&self.tmp[..n]);
        }
        self.buf[0]
    }

    /// Delay the round trip adds, in stream-rate samples.
    pub fn latency_samples(&self) -> usize {
        self.latency
    }
}

/// A mono processor built for, and run at, `factor` × the stream rate.
pub struct OversampledMono {
    oversampler: IirOversampler,
    inner: Box<dyn MonoProcessor>,
}

impl OversampledMono {
    /// `build` receives the inner rate.
    pub fn new(
        sample_rate: f32,
        factor: usize,
        build: impl FnOnce(f32) -> Box<dyn MonoProcessor>,
    ) -> Self {
        let oversampler = IirOversampler::new(sample_rate, factor);
        let inner = build(sample_rate * oversampler.factor() as f32);
        Self { oversampler, inner }
    }
}

impl MonoProcessor for OversampledMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        let inner = &mut self.inner;
        self.oversampler.process(input, |s| inner.process_sample(s))
    }

    fn latency_samples(&self) -> usize {
        self.oversampler.latency_samples()
            + self.inner.latency_samples() / self.oversampler.factor()
    }
}

#[cfg(test)]
#[path = "iir_oversampler_tests.rs"]
mod tests;
