//! STFT phase-vocoder pitch shifter — time-stretch + resample.
//!
//! Reusable foundation — `native_pitch_shifter` consumes it now, and Wave 2
//! of issue #381 (native autotune) will reuse the same module. All scratch
//! is pre-allocated in `new()`, so `process_sample` is RT-safe (zero alloc,
//! zero lock).
//!
//! ## Why not the naive Bernsee bin-remap (issue #488)
//!
//! The previous implementation pitch-shifted in the frequency domain by
//! rounding each analysis bin to `round(bin * pitch)` and summing its
//! magnitude there. That collapses every partial's whole main lobe into a
//! single bin; the collapsed magnitude beats at the hop rate as the moving
//! analysis window samples the off-bin sinusoid, producing a periodic
//! amplitude dropout — heard as the note warbling / cutting out on any
//! real (multi-partial) instrument. A pure sine has a stable lobe and was
//! immune, which is why the old peak-bin-only tests never caught it.
//!
//! ## Method (canonical)
//!
//! Pitch shift = **time-stretch by `pitch`** (phase vocoder, no bin
//! remap — partials keep their exact frequency and lobe shape) **then
//! resample by `pitch`** (linear interpolation) to restore the original
//! duration. Net: same length, pitch scaled, lobe shape preserved.
//!
//! The synthesis hop is fixed at `N / OVERLAP_FACTOR` so the Hann² 75%
//! overlap-add stays COLA-correct. The analysis hop is `syn_hop / pitch`
//! input samples (fractional, tracked with an accumulator); the exact
//! integer number of input samples elapsed between two analysis frames is
//! used for the phase-deviation term, so the instantaneous frequency
//! estimate is exact for any pitch ratio.
//!
//! Reference: Bernsee 2003; Dolson, "The Phase Vocoder: A Tutorial" (1986).

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::f32::consts::TAU;
use std::sync::Arc;

pub const MIN_WINDOW_SIZE: usize = 256;
pub const MAX_WINDOW_SIZE: usize = 8192;
pub const OVERLAP_FACTOR: usize = 4;
pub const MIN_PITCH_FACTOR: f32 = 0.25;
pub const MAX_PITCH_FACTOR: f32 = 4.0;

pub struct PhaseVocoder {
    window_size: usize,
    syn_hop: usize,
    bins: usize,
    fft_forward: Arc<dyn RealToComplex<f32>>,
    fft_inverse: Arc<dyn ComplexToReal<f32>>,

    window: Vec<f32>,
    synth_scale: f32,

    in_ring: Vec<f32>,
    in_write: usize,
    samples_since_analysis: usize,
    analysis_accum: f32,
    analysis_step: f32,

    // Stretched-signal output ring. Synthesis overlap-adds here advancing
    // by `syn_hop` per frame; the resampler reads it at fractional step
    // `pitch_factor`. Net write/read rate is equal (`syn_hop` per analysis
    // frame), so a ring of `window_size` keeps a constant lag — the same
    // proven scheme as a plain STFT, with a fractional reader on top.
    out_ring: Vec<f32>,
    out_len: usize,
    out_abs: u64,
    read_pos: f64,
    read_floor: i64,
    primed: bool,

    time_frame: Vec<f32>,
    freq_in: Vec<Complex<f32>>,
    freq_out: Vec<Complex<f32>>,
    fwd_scratch: Vec<Complex<f32>>,
    inv_scratch: Vec<Complex<f32>>,

    prev_phase: Vec<f32>,
    sum_phase: Vec<f32>,

    pitch_factor: f32,
}

impl PhaseVocoder {
    pub fn new(window_size: usize) -> Self {
        let window_size = window_size.clamp(MIN_WINDOW_SIZE, MAX_WINDOW_SIZE);
        assert!(
            window_size.is_power_of_two(),
            "window_size must be a power of two"
        );
        let syn_hop = window_size / OVERLAP_FACTOR;
        let bins = window_size / 2 + 1;

        let mut planner = RealFftPlanner::<f32>::new();
        let fft_forward = planner.plan_fft_forward(window_size);
        let fft_inverse = planner.plan_fft_inverse(window_size);

        let window: Vec<f32> = (0..window_size)
            .map(|n| 0.5 - 0.5 * (TAU * n as f32 / window_size as f32).cos())
            .collect();

        // Hann analysis × Hann synthesis, 75% overlap (syn_hop = N/4):
        // Σ window² over the 4 overlapping frames = 1.5, constant. realfft
        // IFFT is unnormalised, so divide by N too.
        let synth_scale = 2.0 / (3.0 * window_size as f32);

        let fwd_scratch = vec![Complex::new(0.0, 0.0); fft_forward.get_scratch_len()];
        let inv_scratch = vec![Complex::new(0.0, 0.0); fft_inverse.get_scratch_len()];

        Self {
            window_size,
            syn_hop,
            bins,
            fft_forward,
            fft_inverse,
            window,
            synth_scale,
            in_ring: vec![0.0; window_size],
            in_write: 0,
            samples_since_analysis: 0,
            analysis_accum: 0.0,
            analysis_step: syn_hop as f32, // pitch 1.0 until set
            out_ring: vec![0.0; 2 * window_size],
            out_len: 2 * window_size,
            out_abs: 0,
            read_pos: 0.0,
            read_floor: 0,
            primed: false,
            time_frame: vec![0.0; window_size],
            freq_in: vec![Complex::new(0.0, 0.0); bins],
            freq_out: vec![Complex::new(0.0, 0.0); bins],
            fwd_scratch,
            inv_scratch,
            prev_phase: vec![0.0; bins],
            sum_phase: vec![0.0; bins],
            pitch_factor: 1.0,
        }
    }

    pub fn set_pitch_factor(&mut self, factor: f32) {
        self.pitch_factor = factor.clamp(MIN_PITCH_FACTOR, MAX_PITCH_FACTOR);
        // Analysis hop in input samples: synthesising at the fixed COLA
        // hop while consuming `syn_hop / pitch` inputs per frame stretches
        // the signal by exactly `pitch` (same pitch, longer/shorter), which
        // the resampler then reads back at step `pitch`.
        self.analysis_step = self.syn_hop as f32 / self.pitch_factor;
    }

    pub fn process_sample(&mut self, input: f32) -> f32 {
        self.in_ring[self.in_write] = input;
        self.in_write = (self.in_write + 1) % self.window_size;
        self.samples_since_analysis += 1;
        self.analysis_accum += 1.0;

        if self.analysis_accum >= self.analysis_step {
            self.analysis_accum -= self.analysis_step;
            let elapsed = self.samples_since_analysis.max(1);
            self.samples_since_analysis = 0;
            self.process_frame(elapsed);
            self.out_abs += self.syn_hop as u64;
        }

        self.resample_output()
    }

    /// Read the stretched ring at the fractional read pointer (linear
    /// interpolation), advancing it by `pitch_factor` per output sample.
    /// Consumed integer slots are zeroed so the ring stays clean for the
    /// overlap-add accumulator to reuse — the same zero-after-consume
    /// scheme a plain STFT uses, generalised to a fractional step.
    fn resample_output(&mut self) -> f32 {
        // Read this many samples behind the write head. A sample stops
        // receiving overlap-add contributions once the writer has moved a
        // full window past it, so `window - syn_hop` of lag guarantees all
        // OVERLAP_FACTOR contributions are in. Plus one analysis window of
        // fill before the first valid sample exists.
        let lead = self.window_size - self.syn_hop;
        if !self.primed {
            if self.out_abs < (self.window_size + lead) as u64 {
                return 0.0;
            }
            self.primed = true;
            let start = (self.out_abs - lead as u64) as f64;
            self.read_pos = start;
            self.read_floor = start as i64;
        }

        let len = self.out_len as i64;
        let i0 = self.read_pos.floor() as i64;
        let frac = (self.read_pos - i0 as f64) as f32;
        let a = self.out_ring[(i0.rem_euclid(len)) as usize];
        let b = self.out_ring[((i0 + 1).rem_euclid(len)) as usize];
        let y = a + (b - a) * frac;

        let next = self.read_pos + self.pitch_factor as f64;
        // Zero every integer slot we have fully passed (strictly before
        // the new floor) so it is clean when the writer laps back to it.
        let new_floor = next.floor() as i64;
        let mut z = self.read_floor;
        while z < new_floor {
            self.out_ring[(z.rem_euclid(len)) as usize] = 0.0;
            z += 1;
        }
        self.read_floor = new_floor;
        self.read_pos = next;
        y
    }

    fn process_frame(&mut self, elapsed: usize) {
        for i in 0..self.window_size {
            let idx = (self.in_write + i) % self.window_size;
            self.time_frame[i] = self.in_ring[idx] * self.window[i];
        }

        if self
            .fft_forward
            .process_with_scratch(
                &mut self.time_frame,
                &mut self.freq_in,
                &mut self.fwd_scratch,
            )
            .is_err()
        {
            return;
        }

        self.repitch_bins(elapsed);

        if self
            .fft_inverse
            .process_with_scratch(
                &mut self.freq_out,
                &mut self.time_frame,
                &mut self.inv_scratch,
            )
            .is_err()
        {
            return;
        }

        for i in 0..self.window_size {
            let pos = ((self.out_abs as usize) + i) % self.out_len;
            self.out_ring[pos] += self.time_frame[i] * self.window[i] * self.synth_scale;
        }
    }

    /// Phase-vocoder resynthesis with NO bin remap. Each bin keeps its own
    /// estimated instantaneous frequency; the synthesis phase advances by
    /// that frequency over the fixed synthesis hop. The output is the
    /// input time-stretched by `pitch_factor` (same pitch, lobes intact);
    /// the resampler applies the actual pitch change. `elapsed` is the
    /// exact integer number of input samples since the previous analysis
    /// frame — used so the phase-deviation term is exact for any ratio.
    fn repitch_bins(&mut self, elapsed: usize) {
        let elapsed_f = elapsed as f32;
        let n = self.window_size as f32;
        let syn = self.syn_hop as f32;

        for k in 0..self.bins {
            let bin = self.freq_in[k];
            let magn = bin.norm();
            let phase = bin.arg();

            // Expected phase advance of bin k over `elapsed` input samples.
            let expected = TAU * k as f32 * elapsed_f / n;
            let mut delta = phase - self.prev_phase[k] - expected;
            self.prev_phase[k] = phase;

            // Wrap to [-π, π].
            let cycles = (delta / TAU).round();
            delta -= TAU * cycles;

            // Instantaneous frequency of bin k, in rad per input sample.
            let true_freq = (k as f32 + delta * n / (TAU * elapsed_f)) * TAU / n;

            // Advance the synthesis phase over the fixed synthesis hop.
            self.sum_phase[k] += true_freq * syn;
            let p = self.sum_phase[k];
            // DC (k=0) and Nyquist (k=bins-1) of a real signal are real;
            // realfft's ComplexToReal rejects a non-zero imaginary part
            // there. Collapse those two to a real value, keep the rest as
            // the full magnitude/phase phasor.
            self.freq_out[k] = if k == 0 || k + 1 == self.bins {
                Complex::new(magn * p.cos(), 0.0)
            } else {
                Complex::new(magn * p.cos(), magn * p.sin())
            };
        }
    }
}

#[cfg(test)]
#[path = "phase_vocoder_tests.rs"]
mod tests;
