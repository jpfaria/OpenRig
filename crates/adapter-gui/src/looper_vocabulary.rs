//! Responsibility: maps a looper's state onto what the row displays.

use engine::LooperState;
use project::chain::LooperSpeed;

pub(crate) fn state_code(state: LooperState) -> i32 {
    match state {
        LooperState::Empty => 0,
        LooperState::Recording => 1,
        LooperState::Playing => 2,
        LooperState::Overdubbing => 3,
        LooperState::Stopped => 4,
    }
}

pub(crate) fn speed_index(speed: LooperSpeed) -> i32 {
    match speed {
        LooperSpeed::Half => 0,
        LooperSpeed::Normal => 1,
        LooperSpeed::Double => 2,
    }
}

/// "m:ss" of a frame count at the stream's LIVE rate — never a hardcoded
/// 48000 (#669/#723: a 44.1 kHz stream would read 9 % fast).
/// #826: the same clock the panel shows, for the waveform editor's header.
pub(crate) fn clock_label(frames: usize, sample_rate: u32) -> String {
    clock(frames, sample_rate)
}

pub(crate) fn clock(frames: usize, sample_rate: u32) -> String {
    let seconds = frames as f64 / f64::from(sample_rate.max(1));
    let total = seconds.floor() as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

/// Rows for one chain's loopers. `recorded` carries, per looper, how many
/// layers exist including the ones an undo silenced — that is what makes redo
/// available; pass an empty slice when it is not known.
/// #323 phase 2: the option index a loop's linked preset maps to, for the
/// drawer button + picker. `preset_ids[k]` is the id of option `k + 1` (option
/// 0 is "follow the chain"), so a linked id present in the bank is `pos + 1` and
/// an absent / `None` link is 0 (follow).
pub(crate) fn preset_option_index(linked: Option<&str>, preset_ids: &[String]) -> i32 {
    match linked {
        Some(id) => preset_ids
            .iter()
            .position(|p| p == id)
            .map(|pos| pos as i32 + 1)
            .unwrap_or(0),
        None => 0,
    }
}
