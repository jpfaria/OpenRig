//! Responsibility: maps a tempo-sync note value to a delay time or a modulation rate.
//! Tempo sync for native delays (`time_ms`) and modulations (`rate_hz`).
//!
//! A synced block stores the note value in a select param (`time_sync` /
//! `rate_sync`); the dispatcher derives the plain value from the global BPM
//! off the audio thread and writes it into the block, so the DSP never learns
//! about tempo. `off` keeps the knob free.

use crate::param::{enum_parameter, ParameterSpec};

pub const TIME_SYNC_PATH: &str = "time_sync";
pub const RATE_SYNC_PATH: &str = "rate_sync";
pub const TIME_PATH: &str = "time_ms";
pub const RATE_PATH: &str = "rate_hz";
pub const SYNC_OFF: &str = "off";

/// `(value, label)`; `d` = dotted, `t` = triplet.
pub const SYNC_OPTIONS: &[(&str, &str)] = &[
    (SYNC_OFF, "Off"),
    ("1/1", "1/1"),
    ("1/2", "1/2"),
    ("1/2d", "1/2."),
    ("1/2t", "1/2T"),
    ("1/4", "1/4"),
    ("1/4d", "1/4."),
    ("1/4t", "1/4T"),
    ("1/8", "1/8"),
    ("1/8d", "1/8."),
    ("1/8t", "1/8T"),
    ("1/16", "1/16"),
    ("1/16d", "1/16."),
    ("1/16t", "1/16T"),
];

/// Length of a note value in quarter notes; `None` for `off` or unknown.
pub fn sync_beats(value: &str) -> Option<f32> {
    let (base, modifier) = match value.strip_suffix('d') {
        Some(base) => (base, 1.5),
        None => match value.strip_suffix('t') {
            Some(base) => (base, 2.0 / 3.0),
            None => (value, 1.0),
        },
    };
    let whole_fraction = match base {
        "1/1" => 1.0,
        "1/2" => 0.5,
        "1/4" => 0.25,
        "1/8" => 0.125,
        "1/16" => 0.0625,
        _ => return None,
    };
    Some(whole_fraction * 4.0 * modifier)
}

/// Delay time of a note value lasting `beats` quarter notes at `bpm`.
pub fn synced_time_ms(bpm: f32, beats: f32) -> f32 {
    60_000.0 / bpm * beats
}

/// Modulation rate whose period is a note value of `beats` quarter notes.
pub fn synced_rate_hz(bpm: f32, beats: f32) -> f32 {
    bpm / 60.0 / beats
}

pub fn time_sync_parameter() -> ParameterSpec {
    enum_parameter(
        TIME_SYNC_PATH,
        "Time Sync",
        None,
        Some(SYNC_OFF),
        SYNC_OPTIONS,
    )
}

pub fn rate_sync_parameter() -> ParameterSpec {
    enum_parameter(
        RATE_SYNC_PATH,
        "Rate Sync",
        None,
        Some(SYNC_OFF),
        SYNC_OPTIONS,
    )
}

/// The plain param a sync select drives.
pub fn synced_value_path(sync_path: &str) -> Option<&'static str> {
    match sync_path {
        TIME_SYNC_PATH => Some(TIME_PATH),
        RATE_SYNC_PATH => Some(RATE_PATH),
        _ => None,
    }
}

/// The sync select that drives a plain param.
pub fn sync_path_for_value(value_path: &str) -> Option<&'static str> {
    match value_path {
        TIME_PATH => Some(TIME_SYNC_PATH),
        RATE_PATH => Some(RATE_SYNC_PATH),
        _ => None,
    }
}

#[cfg(test)]
#[path = "tempo_sync_tests.rs"]
mod tests;
