//! 2× oversampling via half-band FIR.
//!
//! Why: any audio non-linearity (saturation, ring-mod, fold, clipper)
//! generates harmonics. At 44.1 kHz, harmonics above 22.05 kHz alias
//! back into the audible range as harsh, in-harmonic artefacts (the
//! signature "DSP buzz" of cheap distortion plugins).
//!
//! Standard fix: process the non-linearity at 2× the sample rate, then
//! filter out everything above the original Nyquist before decimating
//! back. The filter is a steep low-pass at ~0.45 × Nyquist.
//!
//! Implementation: Hamming-windowed sinc, length 31, normalised so that
//! DC gain = 1 (after the 2× zero-stuffing compensation). The
//! coefficients are computed once at construction time and stored in
//! the struct — `cargo build` keeps everything stack-allocated.
//!
//! RT-safe after construction: no allocation, no syscall.

use std::f32::consts::{PI, TAU};

const HBF_LEN: usize = 31;
const HBF_CENTER: i32 = (HBF_LEN as i32) / 2;

fn build_hbf() -> [f32; HBF_LEN] {
    let mut taps = [0.0_f32; HBF_LEN];
    // Half-band low-pass: cutoff at fs/4 of the *up-rate* (= original
    // Nyquist), so sinc argument is d * 0.5.
    for n in 0..HBF_LEN {
        let d = n as i32 - HBF_CENTER;
        let raw = if d == 0 {
            // Limit of sinc as arg -> 0
            0.5
        } else {
            let arg = d as f32 * 0.5;
            (PI * arg).sin() / (PI * arg) * 0.5
        };
        // Hamming window
        let w = 0.54 - 0.46 * ((TAU * n as f32) / (HBF_LEN as f32 - 1.0)).cos();
        taps[n] = raw * w;
    }
    // Normalise so the DC gain is exactly 1 *after* the post-upsample
    // ×2 compensation. In a half-band kernel the centre tap (= 0.5
    // before scaling) carries one of the two output phases, the
    // remaining odd-distance taps carry the other phase. Both phases
    // contribute 0.5 each when total taps sum to 1.0 → after the ×2
    // boost in `up()` the round-trip DC gain is exactly 1.
    let sum: f32 = taps.iter().sum();
    let scale = 1.0 / sum;
    for t in taps.iter_mut() {
        *t *= scale;
    }
    taps
}

/// 2× oversampler. Use as:
///
/// ```ignore
/// let mut os = Oversampler2x::new();
/// for &x in input {
///     let [a, b] = os.up(x);          // 2 samples at 2 fs
///     let a2 = saturate(a);            // non-linear processing
///     let b2 = saturate(b);
///     let y = os.down([a2, b2]);       // back to fs, anti-aliased
///     output.push(y);
/// }
/// ```
pub struct Oversampler2x {
    taps: [f32; HBF_LEN],
    up_buffer: [f32; HBF_LEN],
    up_idx: usize,
    down_buffer: [f32; HBF_LEN],
    down_idx: usize,
}

impl Default for Oversampler2x {
    fn default() -> Self {
        Self::new()
    }
}

impl Oversampler2x {
    pub fn new() -> Self {
        Self {
            taps: build_hbf(),
            up_buffer: [0.0; HBF_LEN],
            up_idx: 0,
            down_buffer: [0.0; HBF_LEN],
            down_idx: 0,
        }
    }

    fn fir(taps: &[f32; HBF_LEN], buffer: &[f32; HBF_LEN], write_idx: usize) -> f32 {
        let mut acc = 0.0_f32;
        for k in 0..HBF_LEN {
            let bi = (write_idx + HBF_LEN - 1 - k) % HBF_LEN;
            acc += taps[k] * buffer[bi];
        }
        acc
    }

    /// Push one sample at the original rate. Returns 2 samples at 2× rate.
    /// Compensates the zero-stuffing energy loss with a ×2 gain.
    pub fn up(&mut self, x: f32) -> [f32; 2] {
        // Phase A: insert input sample.
        self.up_buffer[self.up_idx] = x;
        self.up_idx = (self.up_idx + 1) % HBF_LEN;
        let a = Self::fir(&self.taps, &self.up_buffer, self.up_idx) * 2.0;

        // Phase B: insert zero between every input sample.
        self.up_buffer[self.up_idx] = 0.0;
        self.up_idx = (self.up_idx + 1) % HBF_LEN;
        let b = Self::fir(&self.taps, &self.up_buffer, self.up_idx) * 2.0;

        [a, b]
    }

    /// Push 2 samples at 2× rate. Returns 1 sample at the original rate
    /// after low-pass + decimation.
    pub fn down(&mut self, frames: [f32; 2]) -> f32 {
        // Push both samples through the LPF; only return the second
        // (decimation by 2 keeps every other output).
        self.down_buffer[self.down_idx] = frames[0];
        self.down_idx = (self.down_idx + 1) % HBF_LEN;
        let _ = Self::fir(&self.taps, &self.down_buffer, self.down_idx);

        self.down_buffer[self.down_idx] = frames[1];
        self.down_idx = (self.down_idx + 1) % HBF_LEN;
        Self::fir(&self.taps, &self.down_buffer, self.down_idx)
    }

    /// Reset all internal state. Call between disconnected audio
    /// streams (e.g. on sample-rate change).
    pub fn reset(&mut self) {
        self.up_buffer = [0.0; HBF_LEN];
        self.down_buffer = [0.0; HBF_LEN];
        self.up_idx = 0;
        self.down_idx = 0;
    }

    /// Group delay in original-rate samples (for plugin-side latency
    /// reporting). Linear-phase FIR of length N has group delay
    /// (N-1)/2 at the up-rate, so half of that at base rate.
    pub const fn latency_samples(&self) -> usize {
        (HBF_LEN - 1) / 2 / 2
    }
}

#[cfg(test)]
#[path = "oversampling_tests.rs"]
mod tests;
