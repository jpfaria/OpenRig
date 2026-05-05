//! Stereo-linked brick wall limiter. Used when the chain layout is Stereo so
//! that L and R receive identical gain reduction, preserving the stereo image.

use block_core::StereoProcessor;

use super::gain::{db_to_lin, GainComputer, GainConfig};
use super::lookahead::LookaheadBuffer;
use super::params::LimiterParams;

pub struct BrickWallLimiterStereo {
    lookahead_l: LookaheadBuffer,
    lookahead_r: LookaheadBuffer,
    gain: GainComputer,
    cfg: GainConfig,
    ceiling_lin: f32,
}

impl BrickWallLimiterStereo {
    pub fn new(params: LimiterParams, sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let lookahead_samples = ((params.lookahead_ms * 0.001) * sr).round().max(1.0) as usize;
        Self {
            lookahead_l: LookaheadBuffer::new(lookahead_samples),
            lookahead_r: LookaheadBuffer::new(lookahead_samples),
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

impl StereoProcessor for BrickWallLimiterStereo {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let delayed_l = self.lookahead_l.push(input[0]);
        let delayed_r = self.lookahead_r.push(input[1]);
        // Stereo-linked peak: the louder channel governs the gain applied to
        // both. Without this, L/R would compress independently and the stereo
        // image would shift whenever one channel pushed harder than the other.
        let peak = self.lookahead_l.peak().max(self.lookahead_r.peak());
        let g = self.gain.tick(peak, &self.cfg);
        [
            (delayed_l * g).clamp(-self.ceiling_lin, self.ceiling_lin),
            (delayed_r * g).clamp(-self.ceiling_lin, self.ceiling_lin),
        ]
    }
}

#[cfg(test)]
#[path = "stereo_tests.rs"]
mod tests;
