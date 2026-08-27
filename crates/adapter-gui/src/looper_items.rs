//! Responsibility: builds the looper items the panel lists.

use crate::LooperItem;
use engine::{LooperState, LooperStatus};
use project::chain::Chain;

use crate::looper_vocabulary::{clock, preset_option_index, speed_index, state_code};

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
                // #826: the waveform editor opens on a STOPPED loop that holds
                // material — the two conditions the store itself gates on, so
                // an enabled button is never a lie. The rule is resolved here,
                // not in Slint: the view reads a flag, it does not decide.
                can_edit: state == LooperState::Stopped && len > 0,
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
    looper_items_with_recorded(
        chain,
        statuses,
        sample_rate,
        &[],
        registry,
        runtime_live,
        preset_ids,
    )
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
