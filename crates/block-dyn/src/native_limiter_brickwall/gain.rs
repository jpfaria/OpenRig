//! Gain-reduction computer: peak (linear) → linear gain.
//!
//! Implements a soft-knee brick wall characteristic with instant attack and
//! log-domain release. Internal state is tracked in dB to get a perceptually
//! natural release curve; output is converted to linear scalar at the end of
//! each sample.

const LN_10_OVER_20: f32 = 0.115_129_25;
const LN_9: f32 = 2.197_224_6;
const MIN_LIN: f32 = 1e-10;

pub fn db_to_lin(db: f32) -> f32 {
    (db * LN_10_OVER_20).exp()
}

pub fn lin_to_db(lin: f32) -> f32 {
    lin.max(MIN_LIN).ln() / LN_10_OVER_20
}

#[derive(Debug, Clone, Copy)]
pub struct GainConfig {
    pub threshold_db: f32,
    pub knee_db: f32,
    pub release_coef: f32,
}

impl GainConfig {
    /// Build a config from user-facing parameters plus sample rate.
    pub fn new(threshold_db: f32, knee_db: f32, release_ms: f32, sample_rate: f32) -> Self {
        let release_samples = (release_ms * 0.001 * sample_rate).max(1.0);
        let release_coef = 1.0 - (-LN_9 / release_samples).exp();
        Self {
            threshold_db,
            knee_db: knee_db.max(0.0),
            release_coef: release_coef.clamp(0.0, 1.0),
        }
    }
}

#[derive(Debug)]
pub struct GainComputer {
    /// Current gain reduction in dB (≤ 0; 0 = unity).
    gr_db: f32,
}

impl Default for GainComputer {
    fn default() -> Self {
        Self { gr_db: 0.0 }
    }
}

impl GainComputer {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub fn current_gr_db(&self) -> f32 {
        self.gr_db
    }

    /// Compute the linear gain to apply for a peak magnitude (linear).
    /// Call once per sample; state advances the release envelope.
    pub fn tick(&mut self, peak_lin: f32, cfg: &GainConfig) -> f32 {
        let peak_db = lin_to_db(peak_lin);
        let target = target_gr_db(peak_db, cfg.threshold_db, cfg.knee_db);

        // Instant attack (go down immediately), log release (ease back to 0).
        if target < self.gr_db {
            self.gr_db = target;
        } else {
            self.gr_db += (target - self.gr_db) * cfg.release_coef;
        }

        db_to_lin(self.gr_db)
    }
}

/// Target gain reduction (dB, ≤ 0) for a given input peak (dB) under the
/// soft-knee brick wall curve.
fn target_gr_db(peak_db: f32, threshold_db: f32, knee_db: f32) -> f32 {
    let half_knee = knee_db * 0.5;
    let knee_low = threshold_db - half_knee;
    let knee_high = threshold_db + half_knee;

    if peak_db <= knee_low {
        0.0
    } else if peak_db >= knee_high {
        threshold_db - peak_db
    } else if knee_db <= 0.0 {
        // Hard knee degenerate case.
        (threshold_db - peak_db).min(0.0)
    } else {
        // Quadratic knee (standard DAFX form): gr = -(peak - knee_low)² / (2·knee).
        // At peak=knee_low → 0; at peak=knee_high → -knee/2; matches the
        // above-knee line at knee_high (threshold - knee_high = -knee/2).
        let d = peak_db - knee_low;
        -(d * d) / (2.0 * knee_db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn db_round_trip() {
        for db in [-60.0_f32, -12.0, -1.0, 0.0, 3.0, 6.0] {
            let round = lin_to_db(db_to_lin(db));
            assert!(approx(round, db, 1e-3), "db={db} round={round}");
        }
    }

    #[test]
    fn below_knee_produces_no_reduction() {
        assert_eq!(target_gr_db(-20.0, -1.0, 2.0), 0.0);
        assert_eq!(target_gr_db(-2.5, -1.0, 2.0), 0.0); // exactly at knee_low
    }

    #[test]
    fn above_knee_gives_full_reduction() {
        let gr = target_gr_db(5.0, -1.0, 2.0);
        assert!(approx(gr, -6.0, 1e-5), "gr={gr}");
    }

    #[test]
    fn knee_region_is_smooth() {
        // Knee from -2 to 0 around threshold -1, width 2.
        let samples: Vec<f32> = (0..21)
            .map(|i| -2.0 + (i as f32) * 0.1)
            .map(|db| target_gr_db(db, -1.0, 2.0))
            .collect();
        // Monotonically non-increasing.
        for w in samples.windows(2) {
            assert!(w[1] <= w[0] + 1e-5, "non-monotonic: {w:?}");
        }
        // Endpoints match.
        assert!(approx(samples[0], 0.0, 1e-5));
    }

    #[test]
    fn hard_knee_zero_width() {
        assert_eq!(target_gr_db(-2.0, -1.0, 0.0), 0.0);
        assert!(approx(target_gr_db(0.0, -1.0, 0.0), -1.0, 1e-5));
    }

    #[test]
    fn tick_attack_is_instant() {
        let cfg = GainConfig::new(-1.0, 2.0, 100.0, 48_000.0);
        let mut gc = GainComputer::new();
        // Peak well above threshold → target is -5 dB
        let _ = gc.tick(db_to_lin(4.0), &cfg);
        let expected_gr = target_gr_db(4.0, -1.0, 2.0);
        assert!(
            approx(gc.current_gr_db(), expected_gr, 1e-4),
            "gr={} expected={}",
            gc.current_gr_db(),
            expected_gr
        );
    }

    #[test]
    fn tick_release_approaches_unity() {
        let cfg = GainConfig::new(-1.0, 2.0, 50.0, 48_000.0);
        let mut gc = GainComputer::new();
        // Force a large gain reduction.
        let _ = gc.tick(db_to_lin(10.0), &cfg);
        let start_gr = gc.current_gr_db();
        assert!(start_gr < -5.0, "expected heavy reduction, got {start_gr}");
        // After 4 release time constants (≈200 ms at 50 ms release), remaining
        // reduction should be below 0.05 dB for any reasonable starting point.
        let samples = (0.200 * 48_000.0) as usize;
        for _ in 0..samples {
            let _ = gc.tick(0.0, &cfg);
        }
        assert!(
            gc.current_gr_db() > -0.05,
            "expected recovery, got {}",
            gc.current_gr_db()
        );
    }

    #[test]
    fn tick_release_coef_respects_configured_time() {
        // Release ≈ 100 ms: at 48 kHz, after 100 ms we expect ~90% recovery
        // toward 0 dB from the starting GR.
        let cfg = GainConfig::new(-1.0, 0.0, 100.0, 48_000.0);
        let mut gc = GainComputer::new();
        let _ = gc.tick(db_to_lin(10.0), &cfg); // attack to -11 dB
        let start = gc.current_gr_db();
        let samples = (0.100 * 48_000.0) as usize;
        for _ in 0..samples {
            let _ = gc.tick(0.0, &cfg);
        }
        // Should have recovered at least 85% of the distance.
        let recovered = gc.current_gr_db() - start;
        let target_distance = 0.0 - start;
        let fraction = recovered / target_distance;
        assert!(
            fraction > 0.85 && fraction <= 1.0,
            "release recovery fraction {fraction} out of [0.85, 1.0]"
        );
    }

    #[test]
    fn tick_respects_threshold() {
        let cfg = GainConfig::new(-1.0, 0.0, 100.0, 48_000.0);
        let mut gc = GainComputer::new();
        // Peak below threshold → no reduction.
        for _ in 0..100 {
            let g = gc.tick(db_to_lin(-6.0), &cfg);
            assert!(approx(g, 1.0, 1e-4), "g={g} expected 1.0 below threshold");
        }
    }
}
