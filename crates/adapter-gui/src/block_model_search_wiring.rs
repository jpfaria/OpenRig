//! Wiring for the block-model search/choose-by-id callbacks on both the
//! main window and the standalone `BlockEditorWindow`.
//!
//! Refilters the `filtered_block_model_options` VecModel on every keystroke,
//! resolves a clicked model_id back to its index in the full list, and
//! forwards the choice to the existing `on_choose_block_model` slot.

use std::rc::Rc;

use slint::{ComponentHandle, VecModel};

use crate::{AppWindow, BlockEditorWindow, BlockModelPickerItem};

pub(crate) fn wire(
    window: &AppWindow,
    block_editor_window: &BlockEditorWindow,
    block_model_options: Rc<VecModel<BlockModelPickerItem>>,
    filtered_block_model_options: Rc<VecModel<BlockModelPickerItem>>,
) {
    {
        let block_model_options = block_model_options.clone();
        let filtered_block_model_options = filtered_block_model_options.clone();
        window.on_search_block_model(move |text| {
            crate::model_search_wiring::refilter_block_model_options(
                &block_model_options,
                &filtered_block_model_options,
                text.as_str(),
            );
        });
    }
    {
        let block_model_options = block_model_options.clone();
        let weak_window = window.as_weak();
        window.on_choose_block_model_by_id(move |model_id| {
            let Some(idx) = crate::model_search_wiring::resolve_model_id_in_block_options(
                &block_model_options,
                model_id.as_str(),
            ) else {
                log::warn!(
                    "[search] model_id '{}' not found in main window list",
                    model_id
                );
                return;
            };
            log::info!(
                "[search] main window: resolved '{}' → idx {}",
                model_id,
                idx
            );
            if let Some(win) = weak_window.upgrade() {
                win.set_block_drawer_selected_model_index(idx);
                win.invoke_choose_block_model(idx);
            }
        });
    }
    {
        let block_model_options = block_model_options.clone();
        let filtered_block_model_options = filtered_block_model_options.clone();
        block_editor_window.on_search_block_model(move |text| {
            crate::model_search_wiring::refilter_block_model_options(
                &block_model_options,
                &filtered_block_model_options,
                text.as_str(),
            );
        });
    }
    {
        let block_model_options = block_model_options.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        block_editor_window.on_choose_block_model_by_id(move |model_id| {
            let Some(idx) = crate::model_search_wiring::resolve_model_id_in_block_options(
                &block_model_options,
                model_id.as_str(),
            ) else {
                log::warn!(
                    "[search] model_id '{}' not found in always-open block_editor_window list",
                    model_id
                );
                return;
            };
            log::info!(
                "[search] block_editor_window: resolved '{}' → idx {}",
                model_id,
                idx
            );
            if let Some(win) = weak_block_editor_window.upgrade() {
                win.set_block_drawer_selected_model_index(idx);
                win.invoke_choose_block_model(idx);
            }
        });
    }
}
