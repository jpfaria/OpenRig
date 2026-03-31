//! PSOLA (Pitch-Synchronous Overlap-Add) pitch shifting engine.

use std::f32::consts::PI;

const IO_BUFFER_SIZE: usize = 4096;

/// Hanning window value at position `i` of `length` samples.
fn hanning(i: usize, length: usize) -> f32 {
    if length <= 1 {
        return 1.0;
    }
    0.5 * (1.0 - (2.0 * PI * i as f32 / (length - 1) as f32).cos())
}

pub struct PsolaShifter {
    /// Circular input buffer
    input_buf: Box<[f32; IO_BUFFER_SIZE]>,
    /// Circular output (overlap-add) buffer
    output_buf: Box<[f32; IO_BUFFER_SIZE]>,
    /// Write position in input buffer (total samples written)
    in_write: usize,
    /// Read position in output buffer
    out_read: usize,
    /// Write position in output buffer (where next grain is placed)
    out_write: usize,
    /// Sample rate
    sample_rate: f32,
    /// Current detected period in samples (1/frequency)
    current_period: f32,
    /// Accumulated input samples since last grain placement
    input_since_grain: f32,
    /// Fractional accumulator for output period advancement
    out_write_frac: f32,
    /// Input grain center position (advances by input period per grain)
    in_grain_center: usize,
    /// Fractional accumulator for input grain center advancement
    in_grain_frac: f32,
}

impl PsolaShifter {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            input_buf: Box::new([0.0; IO_BUFFER_SIZE]),
            output_buf: Box::new([0.0; IO_BUFFER_SIZE]),
            in_write: 0,
            out_read: 0,
            out_write: 0,
            sample_rate,
            current_period: 0.0,
            input_since_grain: 0.0,
            out_write_frac: 0.0,
            in_grain_center: 0,
            in_grain_frac: 0.0,
        }
    }

    /// Process a block of mono audio with the given pitch shift ratio.
    ///
    /// `shift_ratio` is target_freq / detected_freq.
    /// - ratio > 1.0 = pitch up (grains placed closer)
    /// - ratio < 1.0 = pitch down (grains placed farther)
    /// - ratio == 1.0 = no change
    ///
    /// `detected_freq`: the detected fundamental frequency, or None for pass-through.
    pub fn process_block(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        detected_freq: Option<f32>,
        shift_ratio: f32,
    ) {
        let Some(freq) = detected_freq else {
            // No pitch detected: pass-through
            output[..input.len()].copy_from_slice(input);
            return;
        };

        if freq <= 0.0 || shift_ratio <= 0.0 {
            output[..input.len()].copy_from_slice(input);
            return;
        }

        let period_samples = self.sample_rate / freq;
        self.current_period = period_samples;

        // Grain size: 2x period for good overlap
        let grain_size = (period_samples * 2.0) as usize;
        let grain_size = grain_size.clamp(4, IO_BUFFER_SIZE / 2);

        // Output period: how far apart to place grains in the output
        // To shift pitch up, place grains closer together
        let output_period = (period_samples / shift_ratio).max(1.0);

        // Write input into circular buffer
        for &sample in input {
            self.input_buf[self.in_write] = sample;
            self.in_write = (self.in_write + 1) % IO_BUFFER_SIZE;
        }

        // Place grains at output_period intervals
        self.input_since_grain += input.len() as f32;
        while self.input_since_grain >= output_period {
            self.input_since_grain -= output_period;

            // Extract grain from input buffer centered on in_grain_center
            let center = self.in_grain_center;
            let half_grain = grain_size / 2;

            for i in 0..grain_size {
                let src_idx = (center + IO_BUFFER_SIZE - half_grain + i) % IO_BUFFER_SIZE;
                let dst_idx = (self.out_write + i) % IO_BUFFER_SIZE;
                let window = hanning(i, grain_size);
                self.output_buf[dst_idx] += self.input_buf[src_idx] * window;
            }

            // Advance input grain center by one input period (with fractional accumulation)
            let in_int = period_samples as usize;
            self.in_grain_frac += period_samples - in_int as f32;
            let in_extra = self.in_grain_frac as usize;
            self.in_grain_frac -= in_extra as f32;
            self.in_grain_center = (self.in_grain_center + in_int + in_extra) % IO_BUFFER_SIZE;

            // Advance output write by one output period (with fractional accumulation)
            let out_int = output_period as usize;
            self.out_write_frac += output_period - out_int as f32;
            let out_extra = self.out_write_frac as usize;
            self.out_write_frac -= out_extra as f32;
            self.out_write = (self.out_write + out_int + out_extra) % IO_BUFFER_SIZE;
        }

        // Read from output buffer and clear after reading
        for sample in output.iter_mut().take(input.len()) {
            *sample = self.output_buf[self.out_read];
            self.output_buf[self.out_read] = 0.0;
            self.out_read = (self.out_read + 1) % IO_BUFFER_SIZE;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_when_no_pitch() {
        let sample_rate = 48000.0;
        let mut shifter = PsolaShifter::new(sample_rate);
        let input: Vec<f32> = (0..256).map(|i| (i as f32 * 0.01).sin()).collect();
        let mut output = vec![0.0f32; 256];

        shifter.process_block(&input, &mut output, None, 1.0);

        // Should be pass-through
        for (i, (&inp, &out)) in input.iter().zip(output.iter()).enumerate() {
            assert!(
                (inp - out).abs() < 1e-6,
                "sample {i}: input={inp}, output={out}"
            );
        }
    }
}
