//! Chain metadata (name, instrument) callbacks for the per-instance
//! `ChainEditorWindow`.
//!
//! Wires `on_update_chain_name` and `on_select_instrument`. The chain's I/O is
//! now selected through the binding checklist (`on_toggle_binding`, wired in
//! `chain_editor_save_cancel_callbacks`); the old per-endpoint group editor was
//! removed in #716.

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;

use crate::chain_editor::instrument_index_to_string;
use crate::state::ChainDraft;
use crate::{AppWindow, ChainEditorWindow};

pub(crate) fn wire(
    editor_window: &ChainEditorWindow,
    weak_window: slint::Weak<AppWindow>,
    chain_draft: Rc<RefCell<Option<ChainDraft>>>,
) {
    // on_update_chain_name
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        editor_window.on_update_chain_name(move |value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                draft.name = value.to_string();
                window.set_chain_draft_name(value.clone());
                chain_window.set_chain_name(value);
            }
        });
    }
    // on_select_instrument
    {
        let chain_draft = chain_draft.clone();
        editor_window.on_select_instrument(move |index| {
            let instrument = instrument_index_to_string(index).to_string();
            log::debug!(
                "[select_instrument] index={}, instrument='{}'",
                index,
                instrument
            );
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                draft.instrument = instrument;
                log::debug!(
                    "[select_instrument] draft updated to '{}'",
                    draft.instrument
                );
            } else {
                log::warn!("[select_instrument] no draft to update!");
            }
        });
    }
}
