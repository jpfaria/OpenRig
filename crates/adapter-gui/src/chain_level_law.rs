//! Responsibility: maps chain-owned levels onto the fader's dB scale.
//! #1007: the compact view's MASTER and LOOPER strips use the mixer fader
//! law (dB), but the chain keeps its volume as a percentage (0..200, 100 =
//! unity) and a looper its mix as a linear level (0..1). The bottom of the
//! fader is silence.

use domain::mixer_gain::{GAIN_DB_MAX, GAIN_DB_MIN};

/// Highest chain volume, percent.
const VOLUME_MAX_PERCENT: i32 = 200;

/// Linear level → dB on the fader's range, rounded like the fader law.
fn level_db(level: f32) -> f32 {
    if level <= 0.0 {
        return GAIN_DB_MIN;
    }
    let db = 20.0 * level.log10();
    ((db * 10.0).round() / 10.0).clamp(GAIN_DB_MIN, GAIN_DB_MAX)
}

/// dB → linear level; the bottom of the fader is silence.
fn db_level(gain_db: f32) -> f32 {
    if gain_db <= GAIN_DB_MIN {
        return 0.0;
    }
    10f32.powf(gain_db / 20.0)
}

pub(crate) fn volume_db(percent: f32) -> f32 {
    level_db(percent / 100.0)
}

pub(crate) fn volume_percent(gain_db: f32) -> i32 {
    ((db_level(gain_db) * 100.0).round() as i32).clamp(0, VOLUME_MAX_PERCENT)
}

pub(crate) fn looper_mix_db(mix: f32) -> f32 {
    level_db(mix)
}

pub(crate) fn looper_mix(gain_db: f32) -> f32 {
    db_level(gain_db).min(1.0)
}

#[cfg(test)]
#[path = "chain_level_law_tests.rs"]
mod tests;
