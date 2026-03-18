use std::f32::consts::TAU;
use stage_core::MonoProcessor;

pub struct SineTremolo {
    rate_hz: f32,
    depth: f32,
    sample_rate: f32,
    phase: f32,
}

impl SineTremolo {
    pub fn new(rate_hz: f32, depth: f32, sample_rate: f32) -> Self {
        Self {
            rate_hz,
            depth: depth.clamp(0.0, 1.0),
            sample_rate,
            phase: 0.0,
        }
    }
}

impl MonoProcessor for SineTremolo {
    fn process_sample(&mut self, input: f32) -> f32 {
        let lfo = 0.5 * (1.0 + self.phase.sin());
        let gain = 1.0 - (self.depth * lfo);
        self.phase = (self.phase + (TAU * self.rate_hz / self.sample_rate)).rem_euclid(TAU);
        input * gain
    }
}
