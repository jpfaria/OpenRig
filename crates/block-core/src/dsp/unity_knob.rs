//! Responsibility: maps a 0–100 % knob whose midpoint is unity gain to decibels.

/// Bottom of an amp, preamp or cab output knob, dB.
pub const OUTPUT_KNOB_MIN_DB: f32 = -18.0;
/// Top of an amp, preamp or cab output knob, dB. A saturating amp already
/// peaks near full scale at unity, so the boost stops where the nominal
/// programme still stays under +6 dBFS (#1106).
pub const OUTPUT_KNOB_MAX_DB: f32 = 4.0;

/// `percent` 0 → `min_db`, 50 → 0 dB, 100 → `max_db`, linear in dB on each
/// half. Out-of-range percents clamp.
pub fn unity_knob_db(percent: f32, min_db: f32, max_db: f32) -> f32 {
    let p = percent.clamp(0.0, 100.0);
    if p <= 50.0 {
        min_db * (1.0 - p / 50.0)
    } else {
        max_db * (p - 50.0) / 50.0
    }
}

/// The shared output-knob law of the amp, preamp and cab models.
pub fn output_knob_db(percent: f32) -> f32 {
    unity_knob_db(percent, OUTPUT_KNOB_MIN_DB, OUTPUT_KNOB_MAX_DB)
}

#[cfg(test)]
#[path = "unity_knob_tests.rs"]
mod tests;
