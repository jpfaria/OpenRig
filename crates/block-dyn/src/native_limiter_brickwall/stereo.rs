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
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn sr() -> f32 {
        48_000.0
    }

    fn default_limiter() -> BrickWallLimiterStereo {
        BrickWallLimiterStereo::new(LimiterParams::default(), sr())
    }

    #[test]
    fn silence_produces_silence() {
        let mut lim = default_limiter();
        for _ in 0..1024 {
            let out = lim.process_frame([0.0, 0.0]);
            assert!(out[0].abs() < 1e-5 && out[1].abs() < 1e-5);
        }
    }

    #[test]
    fn stereo_link_preserves_ratio_between_channels() {
        // L hot, R quiet but non-zero. Gain must apply equally to both,
        // preserving the L:R amplitude ratio (image).
        let mut lim = default_limiter();
        // Warmup.
        for _ in 0..1024 {
            let _ = lim.process_frame([0.0, 0.0]);
        }
        let mut ratios = Vec::new();
        for _ in 0..2048 {
            let out = lim.process_frame([2.0, 0.2]);
            if out[0].abs() > 1e-3 && out[1].abs() > 1e-3 {
                ratios.push(out[0] / out[1]);
            }
        }
        // All observed ratios should match the input ratio (10:1) within tolerance.
        for r in &ratios {
            assert!(
                (r - 10.0).abs() < 0.2,
                "L/R ratio drifted: {r}, expected ~10.0"
            );
        }
    }

    #[test]
    fn hot_stereo_signal_stays_below_ceiling() {
        let mut lim = default_limiter();
        let ceiling = lim.ceiling_lin();
        for i in 0..4096 {
            let l = (i as f32 / sr() * 220.0 * TAU).sin() * 2.0;
            let r = (i as f32 / sr() * 330.0 * TAU).sin() * 2.0;
            let out = lim.process_frame([l, r]);
            assert!(
                out[0].abs() <= ceiling + 1e-4 && out[1].abs() <= ceiling + 1e-4,
                "sample {i}: L={} R={} ceiling={ceiling}",
                out[0].abs(),
                out[1].abs()
            );
        }
    }

    #[test]
    fn left_only_transient_reduces_right_equally() {
        // Transient on L only, R silent. R must be reduced by the same factor
        // as L for the duration of the gain reduction.
        let mut lim = default_limiter();
        // Warmup.
        for _ in 0..1024 {
            let _ = lim.process_frame([0.0, 0.0]);
        }
        // Inject a constant R reference tone.
        let mut observed_r_during_gr = Vec::new();
        for i in 0..1024 {
            let l = if i < 64 { 3.0 } else { 0.0 };
            let r = 0.5;
            let out = lim.process_frame([l, r]);
            // While gain reduction is active, R should be below its input level.
            observed_r_during_gr.push(out[1].abs());
        }
        // At some point during the transient window, R must have been pulled down.
        let reduced_any = observed_r_during_gr.iter().any(|&x| x < 0.45);
        assert!(
            reduced_any,
            "stereo link did not reduce R during L transient"
        );
    }
}
