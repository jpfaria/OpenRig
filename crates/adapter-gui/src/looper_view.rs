//! #323 — the pure view model of the looper panel: persisted parameters
//! (project) merged with the live transport state the audio thread publishes,
//! turned into the rows the panel renders.
//!
//! Pure and testable: no Slint window, no runtime handle. The GUI timer calls
//! it and hands the result to the model (the "screen has no business logic"
//! law).

use engine::{LooperState, LooperStatus};
use project::chain::{Chain, LooperSpeed};

use crate::LooperItem;

fn state_code(state: LooperState) -> i32 {
    match state {
        LooperState::Empty => 0,
        LooperState::Recording => 1,
        LooperState::Playing => 2,
        LooperState::Overdubbing => 3,
        LooperState::Stopped => 4,
    }
}

fn speed_index(speed: LooperSpeed) -> i32 {
    match speed {
        LooperSpeed::Half => 0,
        LooperSpeed::Normal => 1,
        LooperSpeed::Double => 2,
    }
}

/// "m:ss" of a frame count at the stream's LIVE rate — never a hardcoded
/// 48000 (#669/#723: a 44.1 kHz stream would read 9 % fast).
fn clock(frames: usize, sample_rate: u32) -> String {
    let seconds = frames as f64 / f64::from(sample_rate.max(1));
    let total = seconds.floor() as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

/// Rows for one chain's loopers. `recorded` carries, per looper, how many
/// layers exist including the ones an undo silenced — that is what makes redo
/// available; pass an empty slice when it is not known.
pub fn looper_items_with_recorded(
    chain: &Chain,
    statuses: &[LooperStatus],
    sample_rate: u32,
    recorded: &[(u64, usize)],
) -> Vec<LooperItem> {
    chain
        .loopers
        .iter()
        .map(|cfg| {
            let live = statuses.iter().find(|s| s.uid == cfg.uid);
            let len = live.map_or(0, |s| s.len_frames);
            let position = live.map_or(0, |s| s.position_frames);
            let layers = live.map_or(0, |s| s.layers);
            let total = recorded
                .iter()
                .find(|(uid, _)| *uid == cfg.uid)
                .map_or(layers, |(_, n)| *n);
            LooperItem {
                uid: cfg.uid as i32,
                state_code: state_code(live.map_or(LooperState::Empty, |s| s.state)),
                progress: if len > 0 {
                    position as f32 / len as f32
                } else {
                    0.0
                },
                time_label: format!(
                    "{} / {}",
                    clock(position, sample_rate),
                    clock(len, sample_rate)
                )
                .into(),
                layers: layers as i32,
                mix: (cfg.mix * 100.0).round() as i32,
                decay: (cfg.decay * 100.0).round() as i32,
                speed_index: speed_index(cfg.speed),
                reverse: cfg.reverse,
                can_undo: layers > 0,
                can_redo: total > layers,
            }
        })
        .collect()
}

/// Rows for one chain's loopers, without redo bookkeeping.
pub fn looper_items(chain: &Chain, statuses: &[LooperStatus], sample_rate: u32) -> Vec<LooperItem> {
    looper_items_with_recorded(chain, statuses, sample_rate, &[])
}

/// Whether any of the chain's loopers is currently making sound — drives the
/// chain-header button's active tint.
pub fn any_looper_active(items: &[LooperItem]) -> bool {
    items
        .iter()
        .any(|i| i.state_code == 1 || i.state_code == 2 || i.state_code == 3)
}

#[cfg(test)]
#[path = "looper_view_tests.rs"]
mod tests;
