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

#[allow(clippy::too_many_arguments)]
pub fn looper_items_with_recorded(
    chain: &Chain,
    statuses: &[LooperStatus],
    sample_rate: u32,
    // Single-take looper (#323): redo is disabled, so the recorded-layer count
    // is no longer needed to decide `can_redo`. Kept in the signature so callers
    // are unchanged.
    _recorded: &[(u64, usize)],
    registry: &[domain::io_binding::IoBinding],
    runtime_live: bool,
    preset_ids: &[String],
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
            let state = live.map_or(LooperState::Empty, |s| s.state);
            LooperItem {
                uid: cfg.uid as i32,
                state_code: state_code(state),
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
                // Single-take looper (#323): no overdub layers, so undo/redo are
                // disabled (the buttons grey out). Stacking is done with separate
                // loopers, not layers on one — this removes the ↺ trap where undo
                // silenced the only recording and playback then did nothing.
                can_undo: false,
                can_redo: false,
                // Single-take (#323): REC starts a recording only on an EMPTY
                // loop (with a LIVE runtime — recording captures the live input
                // through the chain's runtime, which exists only when the chain
                // is on, not merely enabled/cold-starting), and closes an
                // in-progress recording. Once the loop HAS material
                // (Playing/Stopped) REC is disabled — there is no overdub; the
                // user clears to re-record. Play/clear act on recorded material.
                can_record: match state {
                    LooperState::Empty => runtime_live,
                    LooperState::Recording => true,
                    _ => false,
                },
                input_index: resolve_input_segment(chain, registry, cfg.input.as_ref()) as i32,
                output_index: resolve_output_segment(chain, registry, cfg.output.as_ref()) as i32,
                preset_index: preset_option_index(cfg.preset.as_deref(), preset_ids),
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
    runtime_live: bool,
    preset_ids: &[String],
) -> Vec<LooperItem> {
    looper_items_with_recorded(chain, statuses, sample_rate, &[], registry, runtime_live, preset_ids)
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
    preset_ids: &[String],
) -> Vec<LooperItem> {
    // 1 Hz keeps `clock` total-safe; every frame count is 0 so it never shows.
    // No live runtime here (project-open path) ⇒ REC is not armable yet.
    looper_items_with_recorded(chain, &[], 1, &[], registry, false, preset_ids)
}

/// Whether any of the chain's loopers is currently making sound — drives the
/// chain-header button's active tint.
pub fn any_looper_active(items: &[LooperItem]) -> bool {
    items
        .iter()
        .any(|i| i.state_code == 1 || i.state_code == 2 || i.state_code == 3)
}

/// Populate every chain row's looper Record-from / Play-to endpoint options
/// from the live bindings — offline, no device needed, so a chain that is not
/// started still shows its options (mirrors the DI output picker's
/// `apply_di_outputs_to_rows`). Writes a row back only when its options change.
pub fn apply_looper_endpoints_to_rows(
    project_chains: &slint::VecModel<crate::ProjectChainItem>,
    project: &project::project::Project,
    registry: &[domain::io_binding::IoBinding],
) {
    use slint::Model;
    for (idx, chain) in project.chains.iter().enumerate() {
        let Some(mut row) = project_chains.row_data(idx) else {
            continue;
        };
        let (inputs, outputs) =
            project::binding_discovery::chain_endpoint_labels(chain, registry);
        let cur_in: Vec<String> = row.looper_input_options.iter().map(|s| s.to_string()).collect();
        let cur_out: Vec<String> =
            row.looper_output_options.iter().map(|s| s.to_string()).collect();
        if cur_in == inputs && cur_out == outputs {
            continue;
        }
        row.looper_input_options = slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(
            inputs.into_iter().map(slint::SharedString::from).collect::<Vec<_>>(),
        )));
        row.looper_output_options = slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(
            outputs.into_iter().map(slint::SharedString::from).collect::<Vec<_>>(),
        )));
        project_chains.set_row_data(idx, row);
    }
}

/// #323 phase 2: populate every chain row's looper preset options — the chain's
/// bank preset names, in slot order — so the drawer's preset picker lists them
/// even on a chain that is not started (mirrors `apply_looper_endpoints_to_rows`).
/// The "follow the chain" entry is option 0 and is added by the Slint picker
/// (@tr, so it stays translatable); these are the bank names for options 1..N,
/// matching each loop's `preset_index` (0 = follow, k = bank slot k). A non-rig
/// chain gets an empty list. Writes a row back only when its options change.
pub fn apply_looper_presets_to_rows(
    project_chains: &slint::VecModel<crate::ProjectChainItem>,
    project: &project::project::Project,
    rig: Option<&std::cell::RefCell<project::rig::RigProject>>,
) {
    use slint::Model;
    for (idx, chain) in project.chains.iter().enumerate() {
        let Some(mut row) = project_chains.row_data(idx) else {
            continue;
        };
        let options: Vec<String> = match rig {
            Some(rig) => crate::chain_preset_wiring::chain_preset_bank(&chain.id, &rig.borrow())
                .into_iter()
                .map(|(_, label)| label)
                .collect(),
            None => Vec::new(),
        };
        let cur: Vec<String> = row.looper_preset_options.iter().map(|s| s.to_string()).collect();
        if cur == options {
            continue;
        }
        row.looper_preset_options = slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(
            options.into_iter().map(slint::SharedString::from).collect::<Vec<_>>(),
        )));
        project_chains.set_row_data(idx, row);
    }
}

/// #323 phase 2: the bank preset ids (option order) a chain's loopers map their
/// `preset_index` against. Empty for a non-rig chain.
pub(crate) fn chain_preset_ids(
    chain: &Chain,
    rig: Option<&std::cell::RefCell<project::rig::RigProject>>,
) -> Vec<String> {
    match rig {
        Some(rig) => crate::chain_preset_wiring::chain_preset_bank(&chain.id, &rig.borrow())
            .into_iter()
            .map(|(id, _)| id)
            .collect(),
        None => Vec::new(),
    }
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
    preset_ids: &[String],
) {
    use slint::Model;
    let Some(mut row) = project_chains.row_data(index) else {
        return;
    };
    // `Some(rate)` is passed only when the chain has a LIVE runtime (see the
    // callers), so it doubles as the "REC is armable" signal.
    let rows = match sample_rate {
        Some(rate) => looper_items(chain, statuses, rate, registry, true, preset_ids),
        None => looper_items_from_config(chain, registry, preset_ids),
    };
    row.looper_active = any_looper_active(&rows);
    row.loopers = slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(rows)));
    project_chains.set_row_data(index, row);
}

#[cfg(test)]
#[path = "looper_view_tests.rs"]
mod tests;
