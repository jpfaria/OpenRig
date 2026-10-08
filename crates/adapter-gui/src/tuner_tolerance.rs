//! Responsibility: decides when a tuner reading counts as in tune.

/// How far from the tempered note, in cents, a string may sit and still read
/// in tune.
pub const IN_TUNE_CENTS: f32 = 3.0;

/// Whether a reading `cents` away from its note is in tune.
pub fn in_tune(cents: f32) -> bool {
    cents.abs() < IN_TUNE_CENTS
}

#[cfg(test)]
#[path = "tuner_tolerance_tests.rs"]
mod tests;
