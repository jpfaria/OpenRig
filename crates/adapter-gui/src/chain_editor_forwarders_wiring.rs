//! Wiring for the main window's chain-editor delegation forwarder.
//!
//! Re-emits the instrument selection on the active `ChainEditorWindow` so the
//! main view can drive the editor's instrument selector while it is open:
//!
//! - `on_select_chain_instrument` → `invoke_select_instrument`
//!
//! The per-endpoint I/O group forwarders were removed in #716 (the chain now
//! selects I/O via the binding checklist).

use std::cell::RefCell;
use std::rc::Rc;

use crate::{AppWindow, ChainEditorWindow};

pub(crate) fn wire(
    window: &AppWindow,
    chain_editor_window: Rc<RefCell<Option<ChainEditorWindow>>>,
) {
    {
        let chain_editor_window = chain_editor_window.clone();
        window.on_select_chain_instrument(move |index| {
            if let Some(cew) = chain_editor_window.borrow().as_ref() {
                cew.invoke_select_instrument(index);
            }
        });
    }
}
