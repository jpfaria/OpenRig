//! Responsibility: turns the player's numbers into the text its panel shows.
//!
//! Pure functions only, so every label and the two-press loop gesture are
//! testable without a window.

use std::path::Path;

use engine::player::settings::PLAYER_LOOP_MIN_SECONDS;

/// `seconds` as `m:ss`. Minutes keep counting past the hour: a backing track
/// is never that long, and `61:05` reads better than a third field.
pub(crate) fn format_clock(seconds: f64) -> String {
    let whole = if seconds.is_finite() && seconds > 0.0 {
        seconds.floor() as u64
    } else {
        0
    };
    format!("{}:{:02}", whole / 60, whole % 60)
}

/// `position / duration`, as the LCD shows it.
pub(crate) fn time_label(position: f64, duration: f64) -> String {
    format!("{} / {}", format_clock(position), format_clock(duration))
}

/// The speed knob's readout, e.g. `0.75×`.
pub(crate) fn speed_label(speed: f32) -> String {
    format!("{speed:.2}×")
}

/// The pitch knob's readout: `+2 ST`, `-2 ST`, `0 ST`.
pub(crate) fn semitones_label(semitones: f32) -> String {
    let steps = semitones.round() as i32;
    if steps > 0 {
        format!("+{steps} ST")
    } else {
        format!("{steps} ST")
    }
}

/// The loop line under the clock: `1:00 – 2:00` for a set loop, `1:00 –`
/// while only its start is marked, empty otherwise.
pub(crate) fn loop_label(range: Option<(f64, f64)>, mark: Option<f64>) -> String {
    match (mark, range) {
        (Some(start), _) => format!("{} –", format_clock(start)),
        (None, Some((start, end))) => format!("{} – {}", format_clock(start), format_clock(end)),
        (None, None) => String::new(),
    }
}

/// The name a track is listed and shown by: its file name without extension.
pub(crate) fn track_name(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What one press of the LOOP button does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum LoopPress {
    /// Remember `now` as the loop's start and wait for the second press.
    Mark(f64),
    /// Close the loop between the two presses, earliest first.
    Set { start: f64, end: f64 },
}

/// The first press marks where the loop starts, the second closes it. A
/// second press too close to the first to make a loop marks again instead.
pub(crate) fn loop_press(mark: Option<f64>, now: f64) -> LoopPress {
    match mark {
        Some(mark) if (now - mark).abs() >= PLAYER_LOOP_MIN_SECONDS => LoopPress::Set {
            start: mark.min(now),
            end: mark.max(now),
        },
        _ => LoopPress::Mark(now),
    }
}

#[cfg(test)]
#[path = "player_view_tests.rs"]
mod tests;
