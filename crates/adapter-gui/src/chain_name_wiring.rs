//! Responsibility: wires the chain name edit.
//! Wiring for the main window's chain-name edit callback.
//!
//! Mirrors the typed name into the active `ChainDraft` and the window
//! property so the chain editor reflects the latest text.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global};

use crate::chain_draft_name::record_chain_name;
use crate::state::ChainDraft;
use crate::AppWindow;

pub(crate) fn wire(window: &AppWindow, chain_draft: Rc<RefCell<Option<ChainDraft>>>) {
    let weak_window = window.as_weak();
    crate::ChainEditorBridge::get(window).on_update_chain_name(move |value| {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        if record_chain_name(&chain_draft, value.as_str()) {
            crate::ChainEditorBridge::get(&window).set_chain_draft_name(value);
        }
    });
}
