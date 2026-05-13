//! STFT phase vocoder pitch shifter (Bernsee 2003).
//!
//! Reusable foundation — `native_pitch_shifter` consumes it now, and Wave 2
//! of issue #381 (native autotune) will reuse the same module. All scratch
//! is pre-allocated in `new()`, so `process_sample` is RT-safe (zero alloc,
//! zero lock).
//!
//! Reference: Bernsee, S. M. (2003). "Pitch Shifting Using The Fourier
//! Transform". <http://blogs.zynaptiq.com/bernsee/pitch-shifting-using-the-ft/>

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
    hop_size: usize,
    bins: usize,
    fft_forward: Arc<dyn RealToComplex<f32>>,
    fft_inverse: Arc<dyn ComplexToReal<f32>>,

    window: Vec<f32>,
    synth_scale: f32,

    in_ring: Vec<f32>,
    in_write: usize,
    out_ring: Vec<f32>,
    out_read: usize,
    hop_counter: usize,

    time_frame: Vec<f32>,
    freq_in: Vec<Complex<f32>>,
    freq_out: Vec<Complex<f32>>,
    fwd_scratch: Vec<Complex<f32>>,
    inv_scratch: Vec<Complex<f32>>,

    prev_phase: Vec<f32>,
    sum_phase: Vec<f32>,
    new_mag: Vec<f32>,
    new_freq: Vec<f32>,

    pitch_factor: f32,
}

impl PhaseVocoder {
    pub fn new(window_size: usize) -> Self {
        let window_size = window_size.clamp(MIN_WINDOW_SIZE, MAX_WINDOW_SIZE);
        assert!(
            window_size.is_power_of_two(),
            "window_size must be a power of two"
        );
        let hop_size = window_size / OVERLAP_FACTOR;
        let bins = window_size / 2 + 1;

        let mut planner = RealFftPlanner::<f32>::new();
        let fft_forward = planner.plan_fft_forward(window_size);
        let fft_inverse = planner.plan_fft_inverse(window_size);

        let window: Vec<f32> = (0..window_size)
            .map(|n| 0.5 - 0.5 * (TAU * n as f32 / window_size as f32).cos())
            .collect();

        // Hann + 75% overlap: COLA² sum = 1.5. realfft IFFT is not
        // normalized, so divide by N too.
        let synth_scale = 2.0 / (3.0 * window_size as f32);

        let fwd_scratch = vec![Complex::new(0.0, 0.0); fft_forward.get_scratch_len()];
        let inv_scratch = vec![Complex::new(0.0, 0.0); fft_inverse.get_scratch_len()];

        // Prime hop counter so the first FFT cycle fires once the buffer
        // has collected a full window worth of input.
        let initial_hop_counter = hop_size.saturating_sub(window_size % hop_size);

        Self {
            window_size,
            hop_size,
            bins,
            fft_forward,
            fft_inverse,
            window,
            synth_scale,
            in_ring: vec![0.0; window_size],
            in_write: 0,
            out_ring: vec![0.0; window_size],
            out_read: 0,
            hop_counter: initial_hop_counter,
            time_frame: vec![0.0; window_size],
            freq_in: vec![Complex::new(0.0, 0.0); bins],
            freq_out: vec![Complex::new(0.0, 0.0); bins],
            fwd_scratch,
            inv_scratch,
            prev_phase: vec![0.0; bins],
            sum_phase: vec![0.0; bins],
            new_mag: vec![0.0; bins],
            new_freq: vec![0.0; bins],
            pitch_factor: 1.0,
        }
    }

    pub fn set_pitch_factor(&mut self, factor: f32) {
        self.pitch_factor = factor.clamp(MIN_PITCH_FACTOR, MAX_PITCH_FACTOR);
    }

    pub fn process_sample(&mut self, input: f32) -> f32 {
        self.in_ring[self.in_write] = input;
        self.in_write = (self.in_write + 1) % self.window_size;
        self.hop_counter += 1;

        if self.hop_counter >= self.hop_size {
            self.hop_counter = 0;
            self.process_frame();
        }

        let y = self.out_ring[self.out_read];
        self.out_ring[self.out_read] = 0.0;
        self.out_read = (self.out_read + 1) % self.window_size;
        y
    }

    fn process_frame(&mut self) {
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

        self.pitch_shift_bins();

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
            let pos = (self.out_read + i) % self.window_size;
            self.out_ring[pos] += self.time_frame[i] * self.window[i] * self.synth_scale;
        }
    }

    fn pitch_shift_bins(&mut self) {
        let expected_phase_per_bin = TAU * self.hop_size as f32 / self.window_size as f32;

        self.new_mag.fill(0.0);
        self.new_freq.fill(0.0);

        for k in 0..self.bins {
            let bin = self.freq_in[k];
            let magn = bin.norm();
            let phase = bin.arg();

            let mut delta = phase - self.prev_phase[k] - expected_phase_per_bin * k as f32;
            // Wrap delta into [-π, π] by removing full 2π cycles.
            let cycles = (delta / TAU).round();
            delta -= TAU * cycles;
            let deviation = delta * self.window_size as f32 / (self.hop_size as f32 * TAU);
            let true_freq = k as f32 + deviation;

            self.prev_phase[k] = phase;

            let shifted = true_freq * self.pitch_factor;
            let new_bin = shifted.round() as i32;
            if new_bin < 0 || (new_bin as usize) >= self.bins {
                continue;
            }
            let nb = new_bin as usize;
            self.new_mag[nb] += magn;
            self.new_freq[nb] = shifted;
        }

        for k in 0..self.bins {
            let advance = self.new_freq[k] * TAU * self.hop_size as f32 / self.window_size as f32;
            self.sum_phase[k] += advance;
            let phase = self.sum_phase[k];
            self.freq_out[k] =
                Complex::new(self.new_mag[k] * phase.cos(), self.new_mag[k] * phase.sin());
        }
    }
}

#[cfg(test)]
#[path = "phase_vocoder_tests.rs"]
mod tests;
