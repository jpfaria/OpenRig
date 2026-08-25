//! Responsibility: delays a signal by a smoothed number of samples.

use crate::delay_math::{
    calculate_coefficient, clamp_time_ms, read_interpolated, sanitize, MAX_DELAY_MS, SMOOTH_TIME_MS,
};

pub struct DelayLine {
    buffer: Vec<f32>,
    write_pos: usize,
    delay_samples_smoothed: f32,
    delay_samples_target: f32,
    smooth_coeff: f32,
    sample_rate: f32,
}

impl DelayLine {
    pub fn new(initial_time_ms: f32, sample_rate: f32) -> Self {
        let max_samples = (MAX_DELAY_MS * 0.001 * sample_rate) as usize + 2;
        let delay_samples = clamp_time_ms(initial_time_ms) * 0.001 * sample_rate;
        Self {
            buffer: vec![0.0; max_samples],
            write_pos: 0,
            delay_samples_smoothed: delay_samples,
            delay_samples_target: delay_samples,
            smooth_coeff: calculate_coefficient(SMOOTH_TIME_MS, sample_rate),
            sample_rate,
        }
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    pub fn set_delay_ms(&mut self, time_ms: f32) {
        self.delay_samples_target = clamp_time_ms(time_ms) * 0.001 * self.sample_rate;
    }

    pub fn read(&mut self) -> f32 {
        self.delay_samples_smoothed = self.smooth_coeff.mul_add(
            self.delay_samples_smoothed,
            (1.0 - self.smooth_coeff) * self.delay_samples_target,
        );
        read_interpolated(
            &self.buffer,
            self.write_pos,
            self.delay_samples_smoothed.max(1.0),
        )
    }

    pub fn write(&mut self, sample: f32) {
        self.buffer[self.write_pos] = sanitize(sample);
        self.write_pos = (self.write_pos + 1) % self.buffer.len();
    }
}
