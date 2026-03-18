use stage_core::{calculate_coefficient, db_to_lin, EnvelopeFollower, MonoProcessor};

pub struct StudioCleanCompressor {
    attack_ms: f32,
    release_ms: f32,
    threshold: f32,
    ratio: f32,
    makeup: f32,
    mix: f32,
    envelope: EnvelopeFollower,
    sample_rate: f32,
}

impl StudioCleanCompressor {
    pub fn new(
        threshold_db: f32,
        ratio: f32,
        attack_ms: f32,
        release_ms: f32,
        makeup_gain_db: f32,
        mix: f32,
        sample_rate: f32,
    ) -> Self {
        Self {
            attack_ms,
            release_ms,
            threshold: db_to_lin(threshold_db),
            ratio,
            makeup: db_to_lin(makeup_gain_db),
            mix: mix.clamp(0.0, 1.0),
            envelope: EnvelopeFollower::from_ms(attack_ms, release_ms, sample_rate),
            sample_rate,
        }
    }

    pub fn set_attack_ms(&mut self, attack_ms: f32) {
        self.attack_ms = attack_ms;
        self.envelope
            .set_attack_coeff(calculate_coefficient(attack_ms, self.sample_rate));
    }

    pub fn set_release_ms(&mut self, release_ms: f32) {
        self.release_ms = release_ms;
        self.envelope
            .set_release_coeff(calculate_coefficient(release_ms, self.sample_rate));
    }
}

impl MonoProcessor for StudioCleanCompressor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let level_in = input.abs().max(1e-10);
        self.envelope.process(level_in);
        let env = self.envelope.value();
        let over_threshold = (env / self.threshold).max(1.0);
        let gain_reduction = if over_threshold > 1.0 {
            over_threshold.powf((1.0 / self.ratio) - 1.0)
        } else {
            1.0
        };
        let compressed = input * gain_reduction * self.makeup;
        (1.0 - self.mix).mul_add(input, self.mix * compressed)
    }
}
