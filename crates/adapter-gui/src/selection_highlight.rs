//! #591: resolve which chain row + block chip the Chains screen highlights,
//! straight from the dispatcher-owned `SelectionState` (the single source of
//! truth the MIDI footswitch also reads).
//!
//! Before this, the highlight was driven by GUI-local block-click state, so
//! moving the active chain/block via a MIDI footswitch (prev/next) changed
//! the selection invisibly — the user could not tell which chain a
//! `toggle_active_chain_enabled` press would act on. Driving the markers from
//! `SelectionState` keeps screen and footswitch in lock-step.

use application::SelectionState;
use project::project::Project;

use crate::project_view::real_block_index_to_ui;
use crate::AppWindow;

/// `(chain_index, block_ui_index)` to highlight, or `-1` for "none".
///
/// `block_ui_index` is the position in the UI block strip (Input/Output
/// stripped), matching what `selected-chain-block-index` expects.
pub(crate) fn active_highlight_indices(project: &Project, sel: &SelectionState) -> (i32, i32) {
    let Some(active_chain) = sel.active_chain.as_deref() else {
        return (-1, -1);
    };
    let Some(chain_index) = project.chains.iter().position(|c| c.id.0 == active_chain) else {
        // Stale selection (chain removed) — mark nothing rather than a wrong row.
        return (-1, -1);
    };
    let chain = &project.chains[chain_index];

    let block_ui_index = sel
        .active_block
        .as_deref()
        .and_then(|bid| chain.blocks.iter().position(|b| b.id.0 == bid))
        .and_then(|real| real_block_index_to_ui(chain, real))
        .map(|ui| ui as i32)
        .unwrap_or(-1);

    (chain_index as i32, block_ui_index)
}

/// UI block index of the block that `toggle_active_block_neighbor_enabled`
/// would flip — the block immediately AFTER the active one in the chain's
/// raw block list (wraps), mirroring the dispatcher handler exactly. `-1`
/// when there is no active block, the chain has < 2 blocks, or the
/// raw-next block is an Input/Output endpoint (no chip on the strip).
pub(crate) fn active_neighbor_block_ui_index(project: &Project, sel: &SelectionState) -> i32 {
    let Some(active_chain) = sel.active_chain.as_deref() else {
        return -1;
    };
    let Some(chain) = project.chains.iter().find(|c| c.id.0 == active_chain) else {
        return -1;
    };
    if chain.blocks.len() < 2 {
        return -1;
    }
    let Some(active_block) = sel.active_block.as_deref() else {
        return -1;
    };
    let Some(active_raw) = chain.blocks.iter().position(|b| b.id.0 == active_block) else {
        return -1;
    };
    let neighbor_raw = (active_raw + 1) % chain.blocks.len();
    real_block_index_to_ui(chain, neighbor_raw)
        .map(|ui| ui as i32)
        .unwrap_or(-1)
}

/// Push the active chain/block markers onto the Chains screen from the
/// dispatcher-owned `SelectionState`. Called on every path that can change
/// the selection — GUI clicks, taps, and (critically) the MIDI/footswitch
/// drain — so the screen always shows what a footswitch acts on.
pub(crate) fn sync_selection_markers(window: &AppWindow, project: &Project, sel: &SelectionState) {
    let (chain_index, block_ui_index) = active_highlight_indices(project, sel);
    window.set_selected_chain_block_chain_index(chain_index);
    window.set_selected_chain_block_index(block_ui_index);
    window.set_selected_chain_block_neighbor_index(active_neighbor_block_ui_index(project, sel));
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::ids::{BlockId, ChainId, DeviceId};
    use project::block::{
        AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, OutputBlock, OutputEntry,
    };
    use project::chain::{Chain, ChainInputMode, ChainOutputMode};
    use project::param::ParameterSet;

    fn io_block(id: &str, input: bool) -> AudioBlock {
        AudioBlock {
            id: BlockId(id.to_string()),
            enabled: true,
            kind: if input {
                AudioBlockKind::Input(InputBlock {
                    model: "standard".to_string(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("d".to_string()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0],
                    }],
                })
            } else {
                AudioBlockKind::Output(OutputBlock {
                    model: "standard".to_string(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("d".to_string()),
                        mode: ChainOutputMode::default(),
                        channels: vec![0],
                    }],
                })
            },
        }
    }

    fn core_block(id: &str) -> AudioBlock {
        AudioBlock {
            id: BlockId(id.to_string()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "amp".to_string(),
                model: "test".to_string(),
                params: ParameterSet::default(),
            }),
        }
    }

    fn chain(id: &str) -> Chain {
        Chain {
            id: ChainId(id.to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            // Input, b0, b1, Output — UI strip is [b0, b1] (IO stripped).
            blocks: vec![
                io_block("in", true),
                core_block("b0"),
                core_block("b1"),
                io_block("out", false),
            ],
        }
    }

    fn project() -> Project {
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![chain("rig:input-1"), chain("rig:input-3")],
            midi: None,
        }
    }

    #[test]
    fn no_active_chain_marks_nothing() {
        let sel = SelectionState::default();
        assert_eq!(active_highlight_indices(&project(), &sel), (-1, -1));
    }

    #[test]
    fn active_chain_without_block_marks_the_row_only() {
        let mut sel = SelectionState::default();
        sel.active_chain = Some("rig:input-3".to_string());
        // index 1, no block → block UI index -1
        assert_eq!(active_highlight_indices(&project(), &sel), (1, -1));
    }

    #[test]
    fn active_chain_and_block_marks_both_with_ui_block_index() {
        let mut sel = SelectionState::default();
        sel.active_chain = Some("rig:input-1".to_string());
        sel.active_block = Some("b1".to_string());
        // chain 0; "b1" is the 2nd core block → UI index 1 (IO stripped).
        assert_eq!(active_highlight_indices(&project(), &sel), (0, 1));
    }

    #[test]
    fn stale_active_chain_marks_nothing() {
        let mut sel = SelectionState::default();
        sel.active_chain = Some("rig:does-not-exist".to_string());
        assert_eq!(active_highlight_indices(&project(), &sel), (-1, -1));
    }

    // ── neighbor block (the block `toggle_active_block_neighbor_enabled` acts on) ──

    #[test]
    fn neighbor_is_the_next_ui_block() {
        let mut sel = SelectionState::default();
        sel.active_chain = Some("rig:input-1".to_string());
        sel.active_block = Some("b0".to_string()); // UI 0
        // neighbor = the block after the active one → b1 (UI 1)
        assert_eq!(active_neighbor_block_ui_index(&project(), &sel), 1);
    }

    #[test]
    fn neighbor_is_minus_one_when_next_block_is_io() {
        let mut sel = SelectionState::default();
        sel.active_chain = Some("rig:input-1".to_string());
        sel.active_block = Some("b1".to_string()); // last audio block; raw-next is Output
        // The toggle-neighbor command targets the raw-next block (here the
        // Output endpoint), which has no chip on the strip → not markable.
        assert_eq!(active_neighbor_block_ui_index(&project(), &sel), -1);
    }

    #[test]
    fn neighbor_is_none_without_active_block() {
        let mut sel = SelectionState::default();
        sel.active_chain = Some("rig:input-1".to_string());
        assert_eq!(active_neighbor_block_ui_index(&project(), &sel), -1);
    }
}
