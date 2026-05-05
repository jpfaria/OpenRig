//! Single-channel brick wall limiter. Used when the chain layout is Mono.

use block_core::MonoProcessor;

use super::gain::{db_to_lin, GainComputer, GainConfig};
use super::lookahead::LookaheadBuffer;
use super::params::LimiterParams;

pub struct BrickWallLimiterMono {
    lookahead: LookaheadBuffer,
    gain: GainComputer,
    cfg: GainConfig,
    ceiling_lin: f32,
}

impl BrickWallLimiterMono {
    pub fn new(params: LimiterParams, sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let lookahead_samples = ((params.lookahead_ms * 0.001) * sr).round().max(1.0) as usize;
        Self {
            lookahead: LookaheadBuffer::new(lookahead_samples),
            gain: GainComputer::new(),
            cfg: GainConfig::new(params.threshold_db, params.knee_db, params.release_ms, sr),
            ceiling_lin: db_to_lin(params.ceiling_db),
        }
    }

    #[cfg(test)]
    pub(super) fn ceiling_lin(&self) -> f32 {
        self.ceiling_lin
    }
}

impl MonoProcessor for BrickWallLimiterMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        let delayed = self.lookahead.push(input);
        let peak = self.lookahead.peak();
        let g = self.gain.tick(peak, &self.cfg);
        (delayed * g).clamp(-self.ceiling_lin, self.ceiling_lin)
    }
}

#[cfg(test)]
#[path = "mono_tests.rs"]
mod tests;
