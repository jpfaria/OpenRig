//! Pitch shifter using variable-speed playback with overlap-add crossfading.
//!
//! The input audio is written into a ring buffer. A read head traverses
//! the ring at a rate proportional to the desired pitch shift. When the
//! read head drifts too far from the write head, it is snapped back and
//! crossfaded to avoid clicks.

const RING_SIZE: usize = 8192;
const RING_MASK: usize = RING_SIZE - 1; // RING_SIZE must be power of 2

/// Crossfade length in samples when snapping the read head.
const XFADE_LEN: usize = 256;

pub struct PsolaShifter {
    ring: Box<[f32; RING_SIZE]>,
    write_pos: usize,
    /// Fractional read position
    read_pos: f64,
    /// Whether a crossfade is in progress
    xfading: bool,
    /// Old read position during crossfade
    xfade_old_pos: f64,
    /// Progress through crossfade (0..XFADE_LEN)
    xfade_count: usize,
    /// Read rate for old position during crossfade
    xfade_old_rate: f64,
    total_written: usize,
}

impl PsolaShifter {
    pub fn new(_sample_rate: f32) -> Self {
        Self {
            ring: Box::new([0.0; RING_SIZE]),
            write_pos: 0,
            read_pos: 0.0,
            xfading: false,
            xfade_old_pos: 0.0,
            xfade_count: 0,
            xfade_old_rate: 1.0,
            total_written: 0,
        }
    }

    fn read_at(&self, pos: f64) -> f32 {
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
        // No pitch or trivial ratio: pass-through
        if detected_freq.is_none() || shift_ratio <= 0.0 || shift_ratio > 4.0 {
            output[..input.len()].copy_from_slice(input);
            return;
        }

        let rate = shift_ratio as f64;

        for (i, out) in output.iter_mut().take(input.len()).enumerate() {
            // Write input sample
            self.ring[self.write_pos] = input[i];
            self.write_pos = (self.write_pos + 1) & RING_MASK;
            self.total_written += 1;

            // Read from main head
            let main_sample = self.read_at(self.read_pos);

            // If crossfading, blend with old head
            if self.xfading {
                let old_sample = self.read_at(self.xfade_old_pos);
                let t = self.xfade_count as f32 / XFADE_LEN as f32;
                // Linear crossfade: old fades out, new fades in
                *out = old_sample * (1.0 - t) + main_sample * t;

                self.xfade_old_pos += self.xfade_old_rate;
                self.xfade_count += 1;
                if self.xfade_count >= XFADE_LEN {
                    self.xfading = false;
                }
            } else {
                *out = main_sample;
            }

            // Advance read position at shifted rate
            self.read_pos += rate;

            // Check distance between write and read.
            // We want read to stay about RING_SIZE/4 behind write.
            // If it drifts too far or too close, snap back with crossfade.
            let target_delay = RING_SIZE as f64 / 4.0;
            let min_delay = RING_SIZE as f64 / 8.0;
            let max_delay = RING_SIZE as f64 * 3.0 / 8.0;

            let write_f = self.write_pos as f64;
            let delay = (write_f - self.read_pos).rem_euclid(RING_SIZE as f64);

            if delay < min_delay || delay > max_delay {
                if !self.xfading {
                    // Start crossfade: old head continues from current pos
                    self.xfading = true;
                    self.xfade_old_pos = self.read_pos;
                    self.xfade_old_rate = rate;
                    self.xfade_count = 0;
                    // Snap read to target delay behind write
                    self.read_pos = write_f - target_delay;
                }
            }
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

        for (i, (&inp, &out)) in input.iter().zip(output.iter()).enumerate() {
            assert!(
                (inp - out).abs() < 1e-6,
                "sample {i}: input={inp}, output={out}"
            );
        }
    }

    #[test]
    fn produces_nonzero_output() {
        let sample_rate = 48000.0;
        let mut shifter = PsolaShifter::new(sample_rate);
        let freq = 440.0;
        let total = 8192;

        let input: Vec<f32> = (0..total)
            .map(|i| {
                let t = i as f32 / sample_rate;
                0.5 * (2.0 * std::f32::consts::PI * freq * t).sin()
            })
            .collect();

        let mut output = vec![0.0f32; total];
        for start in (0..total).step_by(256) {
            let end = (start + 256).min(total);
            shifter.process_block(&input[start..end], &mut output[start..end], Some(freq), 1.0);
        }

        let last = &output[total * 3 / 4..];
        let rms = (last.iter().map(|s| s * s).sum::<f32>() / last.len() as f32).sqrt();
        assert!(rms > 0.05, "output should not be silent, rms={rms}");
    }

    #[test]
    fn pitch_up_increases_frequency() {
        let sample_rate = 48000.0;
        let mut shifter = PsolaShifter::new(sample_rate);
        let freq = 440.0;
        let ratio = 1.1; // ~1.7 semitones up
        let total = 16384;

        let input: Vec<f32> = (0..total)
            .map(|i| {
                let t = i as f32 / sample_rate;
                0.5 * (2.0 * std::f32::consts::PI * freq * t).sin()
            })
            .collect();

        let mut output = vec![0.0f32; total];
        for start in (0..total).step_by(256) {
            let end = (start + 256).min(total);
            shifter.process_block(
                &input[start..end],
                &mut output[start..end],
                Some(freq),
                ratio as f32,
            );
        }

        // Zero-crossing freq on last quarter
        let last = &output[total * 3 / 4..];
        let mut crossings = Vec::new();
        for i in 1..last.len() {
            if last[i - 1] <= 0.0 && last[i] > 0.0 {
                crossings.push(i);
            }
        }

        if crossings.len() >= 3 {
            let periods: Vec<f32> = crossings.windows(2).map(|w| (w[1] - w[0]) as f32).collect();
            let avg = periods.iter().sum::<f32>() / periods.len() as f32;
            let out_freq = sample_rate / avg;
            // Output should be higher than input
            assert!(
                out_freq > freq,
                "pitch up: output {out_freq:.1}Hz should be > input {freq}Hz"
            );
        }
    }
}
