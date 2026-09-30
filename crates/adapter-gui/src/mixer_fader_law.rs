//! Responsibility: maps a mixer fader position to a gain in dB.
//! #1007: the panel reports where the cap was dragged (0 = bottom, 1 = top)
//! and draws the cap where this law puts the current gain.
//!
//! Two straight segments, like a console: the bottom quarter of the travel
//! covers -60..-30 dB, the rest -30..+12 dB, so unity lands near 79% and the
//! useful range gets most of the throw. Piecewise linear keeps it exactly
//! invertible.

use domain::mixer_gain::{GAIN_DB_MAX, GAIN_DB_MIN};

/// Where the two segments meet.
const KNEE_POSITION: f32 = 0.25;
const KNEE_DB: f32 = -30.0;

pub(crate) fn db_from_position(position: f32) -> f32 {
    let p = position.clamp(0.0, 1.0);
    let db = if p <= KNEE_POSITION {
        GAIN_DB_MIN + p / KNEE_POSITION * (KNEE_DB - GAIN_DB_MIN)
    } else {
        KNEE_DB + (p - KNEE_POSITION) / (1.0 - KNEE_POSITION) * (GAIN_DB_MAX - KNEE_DB)
    };
    ((db * 10.0).round() / 10.0).clamp(GAIN_DB_MIN, GAIN_DB_MAX)
}

pub(crate) fn position_from_db(db: f32) -> f32 {
    let db = db.clamp(GAIN_DB_MIN, GAIN_DB_MAX);
    if db <= KNEE_DB {
        (db - GAIN_DB_MIN) / (KNEE_DB - GAIN_DB_MIN) * KNEE_POSITION
    } else {
        KNEE_POSITION + (db - KNEE_DB) / (GAIN_DB_MAX - KNEE_DB) * (1.0 - KNEE_POSITION)
    }
}

/// "0.0 dB", "-6.0 dB", "+3.5 dB".
pub(crate) fn gain_label(db: f32) -> String {
    if db > 0.0 {
        format!("+{db:.1} dB")
    } else if db == 0.0 {
        "0.0 dB".to_string()
    } else {
        format!("{db:.1} dB")
    }
}

#[cfg(test)]
#[path = "mixer_fader_law_tests.rs"]
mod tests;
