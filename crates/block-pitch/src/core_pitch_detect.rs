//! AMDF pitch detection for real-time audio processing.
//!
//! Adapted from the tuner AMDF algorithm, but runs every buffer on the audio thread.

const BUFFER_SIZE: usize = 2048;
const MIN_FREQ_HZ: f32 = 65.0; // C2
const MAX_FREQ_HZ: f32 = 1000.0; // B5
const RMS_THRESHOLD: f32 = 0.01;

pub struct PitchDetector {
    buffer: [f32; BUFFER_SIZE],
    write_pos: usize,
    filled: bool,
    sample_rate: f32,
}

impl PitchDetector {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            buffer: [0.0; BUFFER_SIZE],
            write_pos: 0,
            filled: false,
            sample_rate,
        }
    }

    /// Feed a single sample into the circular buffer.
    pub fn push_sample(&mut self, sample: f32) {
        self.buffer[self.write_pos] = sample;
        self.write_pos += 1;
        if self.write_pos >= BUFFER_SIZE {
            self.write_pos = 0;
            self.filled = true;
        }
    }

    /// Detect the fundamental frequency of the buffered signal.
    /// Returns `None` if signal is too weak or no clear pitch found.
    pub fn detect(&self) -> Option<f32> {
        if !self.filled {
            return None;
        }

        // Linearize the circular buffer for analysis
        let mut linear = [0.0f32; BUFFER_SIZE];
        let first_part = BUFFER_SIZE - self.write_pos;
        linear[..first_part].copy_from_slice(&self.buffer[self.write_pos..]);
        linear[first_part..].copy_from_slice(&self.buffer[..self.write_pos]);

        // RMS gate
        let rms = (linear.iter().map(|s| s * s).sum::<f32>() / BUFFER_SIZE as f32).sqrt();
        if rms < RMS_THRESHOLD {
            return None;
        }

        let min_period = (self.sample_rate / MAX_FREQ_HZ) as usize;
        let max_period = ((self.sample_rate / MIN_FREQ_HZ) as usize).min(BUFFER_SIZE / 2);

        if min_period >= max_period {
            return None;
        }

        let mut best_period = 0usize;
        let mut min_diff = f32::MAX;
        let len = linear.len();

        for lag in min_period..max_period {
            let mut diff = 0.0f32;
            let count = len - lag;
            for i in 0..count {
                diff += (linear[i] - linear[i + lag]).abs();
            }
            // Normalize by number of compared samples
            let normalized = diff / count as f32;
            if normalized < min_diff {
                min_diff = normalized;
                best_period = lag;
            }
        }

        if best_period > 0 {
            Some(self.sample_rate / best_period as f32)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    #[test]
    fn detect_440hz_sine() {
        let sample_rate = 48000.0;
        let mut detector = PitchDetector::new(sample_rate);
        let freq = 440.0;

        // Fill buffer with a 440Hz sine wave
        for i in 0..(BUFFER_SIZE * 2) {
            let t = i as f32 / sample_rate;
            let sample = 0.5 * (TAU * freq * t).sin();
            detector.push_sample(sample);
        }

        let detected = detector.detect().expect("should detect pitch");
        let error = (detected - freq).abs();
        assert!(
            error < 2.0,
            "detected {detected}Hz, expected {freq}Hz (error {error}Hz)"
        );
    }

    #[test]
    fn silence_returns_none() {
        let mut detector = PitchDetector::new(48000.0);
        for _ in 0..(BUFFER_SIZE * 2) {
            detector.push_sample(0.0);
        }
        assert!(detector.detect().is_none());
    }
}
