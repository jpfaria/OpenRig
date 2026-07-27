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
    registry: &[domain::io_binding::IoBinding],
) -> Vec<LooperItem> {
    use project::binding_discovery::{resolve_input_segment, resolve_output_segment};
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
                input_index: resolve_input_segment(chain, registry, cfg.input.as_ref()) as i32,
                output_index: resolve_output_segment(chain, registry, cfg.output.as_ref()) as i32,
            }
        })
        .collect()
}

/// Rows for one chain's loopers, without redo bookkeeping.
pub fn looper_items(
    chain: &Chain,
    statuses: &[LooperStatus],
    sample_rate: u32,
    registry: &[domain::io_binding::IoBinding],
) -> Vec<LooperItem> {
    looper_items_with_recorded(chain, statuses, sample_rate, &[], registry)
}

/// Rows built from the chain's PERSISTED config alone — no live runtime yet.
///
/// This is the project-open path: the loopers must appear even before any
/// stream exists, so the panel is never falsely empty (the user-reported
/// "reopened project shows no loopers, then Add accumulates to 8" bug). With
/// no live status every looper reads as empty and every frame count is 0, so
/// the time label is `0:00 / 0:00` and no sample rate is involved — passing a
/// live rate through here would be a fiction, so there is deliberately no rate
/// parameter.
pub fn looper_items_from_config(
    chain: &Chain,
    registry: &[domain::io_binding::IoBinding],
) -> Vec<LooperItem> {
    // 1 Hz keeps `clock` total-safe; every frame count is 0 so it never shows.
    looper_items_with_recorded(chain, &[], 1, &[], registry)
}

/// Whether any of the chain's loopers is currently making sound — drives the
/// chain-header button's active tint.
pub fn any_looper_active(items: &[LooperItem]) -> bool {
    items
        .iter()
        .any(|i| i.state_code == 1 || i.state_code == 2 || i.state_code == 3)
}

/// Rebuild the looper rows of one chain-card row in place.
///
/// `sample_rate` is `Some` only when the chain has a live runtime — then the
/// live `statuses` fill state/position/length; `None` means "no stream yet",
/// and the rows come from the persisted config alone (no fictional rate). Used
/// both by the meter timer (live) and by the panel callbacks so an add /
/// remove reflects immediately even on a chain with no running stream — the
/// path that let the config accumulate to the 8-looper cap with an empty
/// panel.
pub fn write_chain_looper_row(
    project_chains: &slint::VecModel<crate::ProjectChainItem>,
    index: usize,
    chain: &Chain,
    statuses: &[LooperStatus],
    sample_rate: Option<u32>,
    registry: &[domain::io_binding::IoBinding],
) {
    use slint::Model;
    let Some(mut row) = project_chains.row_data(index) else {
        return;
    };
    let rows = match sample_rate {
        Some(rate) => looper_items(chain, statuses, rate, registry),
        None => looper_items_from_config(chain, registry),
    };
    row.looper_active = any_looper_active(&rows);
    row.loopers = slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(rows)));
    project_chains.set_row_data(index, row);
}

#[cfg(test)]
#[path = "looper_view_tests.rs"]
mod tests;
