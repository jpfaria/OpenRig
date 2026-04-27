//! Wiring helpers that connect the Slint `ModelSelectWithSearch`
//! component to the `model_search` filter functions.
//!
//! Every consumer (drawer, block panel editor window, compact chain row)
//! needs the same two operations: re-filter on keystroke, and resolve the
//! clicked model_id back to an index in the full list. These helpers
//! keep that logic out of `lib.rs` (already a god file).

use crate::{BlockEditorWindow, BlockModelPickerItem, CompactBlockItem};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::rc::Rc;

/// Drawer / window: re-filter `full` according to `text` and publish into
/// `filtered`. Called from the `search-block-model` callback.
pub(crate) fn refilter_block_model_options(
    full: &Rc<VecModel<BlockModelPickerItem>>,
    filtered: &Rc<VecModel<BlockModelPickerItem>>,
    text: &str,
) {
    let all: Vec<BlockModelPickerItem> = full.iter().collect();
    let result = crate::model_search::filter_models(&all, text);
    filtered.set_vec(result);
}

/// Drawer / window: resolve a model_id to its index in the full list.
/// Returns `None` when the id is not found.
pub(crate) fn resolve_model_id_in_block_options(
    full: &Rc<VecModel<BlockModelPickerItem>>,
    model_id: &str,
) -> Option<i32> {
    full.iter()
        .enumerate()
        .find(|(_, m)| m.model_id.as_str() == model_id)
        .map(|(i, _)| i as i32)
}

/// Standalone `BlockEditorWindow` (the per-block window opened by clicking
/// an existing block) owns its own `block_model_options` /
/// `filtered_block_model_options` pair, independent of the global ones
/// shared by `AppWindow` and the always-open `BlockEditorWindow`. Wires
/// search + choose-by-id callbacks for that local pair.
pub(crate) fn wire_standalone_block_editor_window(
    win: &BlockEditorWindow,
    win_full: Rc<VecModel<BlockModelPickerItem>>,
    win_filtered: Rc<VecModel<BlockModelPickerItem>>,
) {
    {
        let win_full = win_full.clone();
        let win_filtered = win_filtered.clone();
        win.on_search_block_model(move |text| {
            refilter_block_model_options(&win_full, &win_filtered, text.as_str());
        });
    }
    let weak_win = win.as_weak();
    win.on_choose_block_model_by_id(move |model_id| {
        let Some(idx) = resolve_model_id_in_block_options(&win_full, model_id.as_str()) else {
            return;
        };
        if let Some(w) = weak_win.upgrade() {
            w.invoke_choose_block_model(idx);
        }
    });
}

/// Compact view: re-filter the `filtered_models` vector of the compact
/// block at `(chain_idx, block_idx)`. Replaces the row in the
/// `compact_blocks` model so Slint observes the change.
pub(crate) fn refilter_compact_block(
    compact_blocks: &Rc<VecModel<CompactBlockItem>>,
    chain_idx: i32,
    block_idx: i32,
    text: &str,
) {
    let len = compact_blocks.row_count();
    for i in 0..len {
        let Some(item) = compact_blocks.row_data(i) else {
            continue;
        };
        if item.chain_index != chain_idx || item.block_index != block_idx {
            continue;
        }
        let all: Vec<BlockModelPickerItem> = item.models.iter().collect();
        let filtered = crate::model_search::filter_models(&all, text);
        let mut new_item = item;
        new_item.filtered_models = ModelRc::from(Rc::new(VecModel::from(filtered)));
        compact_blocks.set_row_data(i, new_item);
        return;
    }
}

/// Compact view: resolve a model_id to its index within a specific block's
/// `models` list. Returns `None` when the block or id is not found.
pub(crate) fn resolve_model_id_in_compact_block(
    compact_blocks: &Rc<VecModel<CompactBlockItem>>,
    chain_idx: i32,
    block_idx: i32,
    model_id: &str,
) -> Option<i32> {
    let len = compact_blocks.row_count();
    for i in 0..len {
        let Some(item) = compact_blocks.row_data(i) else {
            continue;
        };
        if item.chain_index != chain_idx || item.block_index != block_idx {
            continue;
        }
        return item
            .models
            .iter()
            .enumerate()
            .find(|(_, m)| m.model_id.as_str() == model_id)
            .map(|(i, _)| i as i32);
    }
    None
}
