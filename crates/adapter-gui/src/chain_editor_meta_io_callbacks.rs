//! Responsibility: handles the edits to a chain's metadata.
//! Chain metadata (name, instrument) callbacks for the per-instance
//! `ChainEditorWindow`.
//!
//! Wires `on_update_chain_name` and `on_select_instrument`. The chain's I/O is
//! now selected through the binding checklist (`on_toggle_binding`, wired in
//! `chain_editor_save_cancel_callbacks`); the old per-endpoint group editor was
//! removed in #716.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global};

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
        crate::ChainEditorBridge::get(editor_window).on_update_chain_name(move |value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            if crate::chain_draft_edits::record_chain_name(&chain_draft, value.as_str()) {
                crate::ChainEditorBridge::get(&window).set_chain_draft_name(value.clone());
                chain_window.set_chain_name(value);
            }
        });
    }
    // on_select_instrument
    {
        let chain_draft = chain_draft.clone();
        editor_window.on_select_instrument(move |index| {
            crate::chain_draft_edits::record_chain_instrument(&chain_draft, index);
        });
    }
}
