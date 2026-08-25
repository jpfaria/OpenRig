//! Responsibility: filters a signal with a one-pole section.

use std::f32::consts::PI;

pub struct OnePoleLowPass {
    state: f32,
    coeff: f32,
}

impl OnePoleLowPass {
    pub fn new(cutoff_hz: f32, sample_rate: f32) -> Self {
        let coeff = 1.0 - (-2.0 * PI * cutoff_hz.max(1.0) / sample_rate).exp();
        Self { state: 0.0, coeff }
    }

    pub fn process(&mut self, input: f32) -> f32 {
        self.state = self.coeff.mul_add(input - self.state, self.state);
        self.state
    }
}

pub struct OnePoleHighPass {
    prev_input: f32,
    prev_output: f32,
    coeff: f32,
}

impl OnePoleHighPass {
    pub fn new(cutoff_hz: f32, sample_rate: f32) -> Self {
        let rc = 1.0 / (2.0 * PI * cutoff_hz.max(1.0));
        let dt = 1.0 / sample_rate;
        let coeff = rc / (rc + dt);
        Self {
            prev_input: 0.0,
            prev_output: 0.0,
            coeff,
        }
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let output = self.coeff * (self.prev_output + input - self.prev_input);
        self.prev_input = input;
        self.prev_output = output;
        output
    }
}
