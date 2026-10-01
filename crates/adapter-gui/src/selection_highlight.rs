//! Responsibility: resolves what the chains screen highlights.
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
use slint::Global;

use crate::AppWindow;

/// `(chain_index, block_ui_index)` to highlight, or `-1` for "none".
///
/// `block_ui_index` is the block's position in `chain.blocks`: the strip draws
/// every entry (model A, #716), so the position IS the chip index (#328).
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
        .map(|position| position as i32)
        .unwrap_or(-1);

    (chain_index as i32, block_ui_index)
}

/// Chip index of the block that `toggle_active_block_neighbor_enabled` would
/// flip — the block immediately AFTER the active one in `chain.blocks` (wraps),
/// mirroring the dispatcher handler exactly. `-1` when there is no active
/// block or the chain has < 2 blocks.
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
    ((active_raw + 1) % chain.blocks.len()) as i32
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
    let (graph_chain, graph_block, graph_neighbor) = graph_selection_ids(project, sel);
    let bridge = crate::ChainGraphBridge::get(window);
    bridge.set_selected_chain_index(graph_chain);
    bridge.set_selected_block_id(graph_block.into());
    bridge.set_neighbor_block_id(graph_neighbor.into());
}

/// #328: `(chain_index, block_id, neighbor_id)` the chain graph marks — the
/// strip's rule above, by block id instead of chip index ("" = none).
pub(crate) fn graph_selection_ids(
    project: &Project,
    sel: &SelectionState,
) -> (i32, String, String) {
    let (chain_index, block_index) = active_highlight_indices(project, sel);
    let neighbor_index = active_neighbor_block_ui_index(project, sel);
    let Some(chain) = usize::try_from(chain_index)
        .ok()
        .and_then(|index| project.chains.get(index))
    else {
        return (-1, String::new(), String::new());
    };
    let id_at = |index: i32| {
        usize::try_from(index)
            .ok()
            .and_then(|i| chain.blocks.get(i))
            .map(|block| block.id.0.clone())
            .unwrap_or_default()
    };
    (chain_index, id_at(block_index), id_at(neighbor_index))
}

#[cfg(test)]
#[path = "selection_highlight_tests.rs"]
mod tests;
