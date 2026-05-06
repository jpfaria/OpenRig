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
#[path = "gain_tests.rs"]
mod tests;
