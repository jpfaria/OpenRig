//! Equalizer stage implementations.
use stage_core::{db_to_lin, MonoProcessor, OnePoleHighPass, OnePoleLowPass};
pub struct ThreeBandEq {
    low_gain: f32,
    mid_gain: f32,
    high_gain: f32,
    low_pass: OnePoleLowPass,
    high_pass: OnePoleHighPass,
}
impl ThreeBandEq {
    pub fn new(
        low_gain_db: f32,
        mid_gain_db: f32,
        high_gain_db: f32,
        sample_rate: f32,
    ) -> Self {
        Self {
            low_gain: db_to_lin(low_gain_db),
            mid_gain: db_to_lin(mid_gain_db),
            high_gain: db_to_lin(high_gain_db),
            low_pass: OnePoleLowPass::new(250.0, sample_rate),
            high_pass: OnePoleHighPass::new(2_000.0, sample_rate),
        }
    }
}
impl MonoProcessor for ThreeBandEq {
    fn process_sample(&mut self, input: f32) -> f32 {
        let low = self.low_pass.process(input);
        let high = self.high_pass.process(input);
        let mid = input - low - high;
        low * self.low_gain + mid * self.mid_gain + high * self.high_gain
    }
}
