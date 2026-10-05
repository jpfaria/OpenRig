//! Responsibility: converts a mixer strip's fader position into a linear gain.

/// Lowest fader position, in dB.
pub const GAIN_DB_MIN: f32 = -60.0;
/// Highest fader position, in dB.
pub const GAIN_DB_MAX: f32 = 12.0;
/// Unity: the default of every strip nobody touched.
pub const GAIN_DB_UNITY: f32 = 0.0;

/// Clamp a requested fader position into the strip range.
pub fn clamp_gain_db(gain_db: f32) -> f32 {
    if gain_db.is_nan() {
        return GAIN_DB_UNITY;
    }
    gain_db.clamp(GAIN_DB_MIN, GAIN_DB_MAX)
}

/// Linear gain the engine multiplies by. Muted = 0; unity is exactly 1.0;
/// the bottom of the fader is silence.
pub fn strip_linear_gain(gain_db: f32, muted: bool) -> f32 {
    let gain_db = clamp_gain_db(gain_db);
    if muted || gain_db <= GAIN_DB_MIN {
        return 0.0;
    }
    if gain_db == GAIN_DB_UNITY {
        return 1.0;
    }
    10.0_f32.powf(gain_db / 20.0)
}
