//! Pitch shifter using windowed overlap-add with resampling.
//!
//! Two overlapping analysis windows read from the input ring at a shifted rate.
//! Each window is Hanning-windowed and the outputs are summed (overlap-add).
//! This produces clean pitch-shifted audio for small shifts (±12 semitones).

use std::f32::consts::PI;

const RING_SIZE: usize = 16384;
const RING_MASK: usize = RING_SIZE - 1;

/// Window size for overlap-add (in samples).
/// Larger = smoother but more latency. 1024 @ 48kHz = ~21ms.
const WINDOW_SIZE: usize = 1024;
const HALF_WINDOW: usize = WINDOW_SIZE / 2;

fn hanning(i: usize, len: usize) -> f32 {
    0.5 * (1.0 - (2.0 * PI * i as f32 / len as f32).cos())
}

pub struct PsolaShifter {
    ring: Box<[f32; RING_SIZE]>,
    write_pos: usize,
    /// Two read heads, offset by half a window for 50% overlap
    read_a: f64,
    read_b: f64,
    /// Phase counter for the overlap-add window (0..WINDOW_SIZE)
    phase: usize,
}

impl PsolaShifter {
    pub fn new(_sample_rate: f32) -> Self {
        Self {
            ring: Box::new([0.0; RING_SIZE]),
            write_pos: 0,
            read_a: 0.0,
            read_b: HALF_WINDOW as f64,
            phase: 0,
        }
    }

    fn read_interp(&self, pos: f64) -> f32 {
        let p = pos.rem_euclid(RING_SIZE as f64);
        let idx = p as usize;
        let frac = (p - idx as f64) as f32;
        let a = self.ring[idx & RING_MASK];
        let b = self.ring[(idx + 1) & RING_MASK];
        a + frac * (b - a)
    }

    pub fn process_block(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        detected_freq: Option<f32>,
        shift_ratio: f32,
    ) {
        // No pitch or trivial: pass-through
        if detected_freq.is_none() || shift_ratio <= 0.0 || shift_ratio > 4.0 {
            output[..input.len()].copy_from_slice(input);
            return;
        }

        let rate = shift_ratio as f64;

        for (i, out) in output.iter_mut().take(input.len()).enumerate() {
            // Write input to ring
            self.ring[self.write_pos] = input[i];
            self.write_pos = (self.write_pos + 1) & RING_MASK;

            // Window weights for the two overlapping heads
            let w_a = hanning(self.phase, WINDOW_SIZE);
            let w_b = hanning((self.phase + HALF_WINDOW) % WINDOW_SIZE, WINDOW_SIZE);

            // Read from both heads with interpolation
            let sample_a = self.read_interp(self.read_a) * w_a;
            let sample_b = self.read_interp(self.read_b) * w_b;

            *out = sample_a + sample_b;

            // Advance both read heads at shifted rate
            self.read_a += rate;
            self.read_b += rate;

            // Advance phase
            self.phase += 1;

            // When head A completes a window, reset it behind write pos
            if self.phase >= WINDOW_SIZE {
                self.phase = 0;
                // Reset head A to start a new window behind the write position
                self.read_a = self.write_pos as f64 - WINDOW_SIZE as f64;
            }

            // At mid-window, reset head B
            if self.phase == HALF_WINDOW {
                self.read_b = self.write_pos as f64 - WINDOW_SIZE as f64;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_when_no_pitch() {
        let mut shifter = PsolaShifter::new(48000.0);
        let input: Vec<f32> = (0..256).map(|i| (i as f32 * 0.01).sin()).collect();
        let mut output = vec![0.0f32; 256];
        shifter.process_block(&input, &mut output, None, 1.0);
        for (i, (&inp, &out)) in input.iter().zip(output.iter()).enumerate() {
            assert!((inp - out).abs() < 1e-6, "sample {i}: in={inp} out={out}");
        }
    }

    #[test]
    fn ratio_one_produces_audio() {
        let mut shifter = PsolaShifter::new(48000.0);
        let freq = 440.0;
        let total = 8192;
        let input: Vec<f32> = (0..total)
            .map(|i| 0.5 * (2.0 * PI * freq * i as f32 / 48000.0).sin())
            .collect();
        let mut output = vec![0.0f32; total];
        for s in (0..total).step_by(256) {
            let e = (s + 256).min(total);
            shifter.process_block(&input[s..e], &mut output[s..e], Some(freq), 1.0);
        }
        let last = &output[total * 3 / 4..];
        let rms = (last.iter().map(|s| s * s).sum::<f32>() / last.len() as f32).sqrt();
        assert!(rms > 0.05, "should produce audio, rms={rms}");
    }

    #[test]
    fn pitch_shift_produces_audio() {
        let mut shifter = PsolaShifter::new(48000.0);
        let freq = 440.0;
        let total = 16384;
        let input: Vec<f32> = (0..total)
            .map(|i| 0.5 * (2.0 * PI * freq * i as f32 / 48000.0).sin())
            .collect();
        let mut output = vec![0.0f32; total];
        for s in (0..total).step_by(256) {
            let e = (s + 256).min(total);
            shifter.process_block(&input[s..e], &mut output[s..e], Some(freq), 1.1);
        }
        let last = &output[total * 3 / 4..];
        let rms = (last.iter().map(|s| s * s).sum::<f32>() / last.len() as f32).sqrt();
        assert!(rms > 0.01, "shifted audio should not be silent, rms={rms}");
    }
}
