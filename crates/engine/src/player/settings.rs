//! Responsibility: defines the backing-track player's playback settings value.
//!
//! A plain value with no DSP state and no device knowledge, so the dispatcher
//! can own it, persist part of it and hand it to whichever runtime plays it.

/// Slowest playback speed the player offers (half speed, pitch preserved).
pub const PLAYER_SPEED_MIN: f32 = 0.5;
/// Fastest playback speed the player offers (double speed, pitch preserved).
pub const PLAYER_SPEED_MAX: f32 = 2.0;
/// Largest transpose, in semitones, either way.
pub const PLAYER_SEMITONES_MAX: f32 = 12.0;
/// Shortest A–B loop the player accepts. Anything shorter is a stutter, not a
/// practice loop, and would spend most of its length inside the crossfade.
pub const PLAYER_LOOP_MIN_SECONDS: f64 = 0.25;

/// How the loaded track plays.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerSettings {
    /// Linear level, `0.0..=1.0`.
    pub volume: f32,
    /// Time ratio, `PLAYER_SPEED_MIN..=PLAYER_SPEED_MAX`; 1.0 is the original.
    pub speed: f32,
    /// Transpose in semitones, `-PLAYER_SEMITONES_MAX..=PLAYER_SEMITONES_MAX`.
    pub semitones: f32,
    /// A–B loop in seconds of the track, `None` when the track plays through.
    pub loop_range: Option<(f64, f64)>,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            volume: 0.8,
            speed: 1.0,
            semitones: 0.0,
            loop_range: None,
        }
    }
}

impl PlayerSettings {
    /// The same settings with every field forced into its supported range. A
    /// loop that is reversed or shorter than [`PLAYER_LOOP_MIN_SECONDS`] is
    /// dropped rather than guessed at.
    pub fn clamped(self) -> Self {
        Self {
            volume: clamp_finite(self.volume, 0.0, 1.0, 0.0),
            speed: clamp_finite(self.speed, PLAYER_SPEED_MIN, PLAYER_SPEED_MAX, 1.0),
            semitones: clamp_finite(
                self.semitones,
                -PLAYER_SEMITONES_MAX,
                PLAYER_SEMITONES_MAX,
                0.0,
            ),
            loop_range: self.loop_range.and_then(valid_loop),
        }
    }

    /// Whether the original audio plays untouched: no time stretch and no
    /// transpose, so the renderer can copy samples instead of running the
    /// stretcher.
    pub fn is_unity(&self) -> bool {
        (self.speed - 1.0).abs() < 1e-4 && self.semitones.abs() < 1e-4
    }
}

fn clamp_finite(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

fn valid_loop((start, end): (f64, f64)) -> Option<(f64, f64)> {
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    let start = start.max(0.0);
    (end - start >= PLAYER_LOOP_MIN_SECONDS).then_some((start, end))
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
