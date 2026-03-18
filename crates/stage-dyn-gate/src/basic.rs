use stage_core::{db_to_lin, EnvelopeFollower, MonoProcessor};

pub struct BasicNoiseGate {
    threshold: f32,
    envelope: EnvelopeFollower,
}

impl BasicNoiseGate {
    pub fn new(threshold_db: f32, attack_ms: f32, release_ms: f32, sample_rate: f32) -> Self {
        Self {
            threshold: db_to_lin(threshold_db),
            envelope: EnvelopeFollower::from_ms(attack_ms, release_ms, sample_rate),
        }
    }
}

impl MonoProcessor for BasicNoiseGate {
    fn process_sample(&mut self, input: f32) -> f32 {
        let env = self.envelope.process(input.abs());
        if env >= self.threshold {
            input
        } else {
            0.0
        }
    }
}
