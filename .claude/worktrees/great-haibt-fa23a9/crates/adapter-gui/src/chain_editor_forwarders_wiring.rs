//! Wiring for the main window's chain-editor delegation forwarders.
//!
//! Seven thin forwarders that re-emit the action on the active
//! `ChainEditorWindow` so the main view can drive the editor's I/O groups
//! and instrument selector while the editor is open:
//!
//! - `on_edit_chain_input`     → `invoke_edit_input`
//! - `on_remove_chain_input`   → `invoke_remove_input`
//! - `on_add_chain_input`      → `invoke_add_input`
//! - `on_edit_chain_output`    → `invoke_edit_output`
//! - `on_remove_chain_output`  → `invoke_remove_output`
//! - `on_add_chain_output`     → `invoke_add_output`
//! - `on_select_chain_instrument` → `invoke_select_instrument`

use std::cell::RefCell;
use std::rc::Rc;

use crate::{AppWindow, ChainEditorWindow};

pub(crate) fn wire(
    window: &AppWindow,
    chain_editor_window: Rc<RefCell<Option<ChainEditorWindow>>>,
) {
    {
        let chain_editor_window = chain_editor_window.clone();
        window.on_edit_chain_input(move |index| {
            if let Some(cew) = chain_editor_window.borrow().as_ref() {
                cew.invoke_edit_input(index);
            }
        });
    }
    {
        let chain_editor_window = chain_editor_window.clone();
        window.on_remove_chain_input(move |index| {
            if let Some(cew) = chain_editor_window.borrow().as_ref() {
                cew.invoke_remove_input(index);
            }
        });
    }
    {
        let chain_editor_window = chain_editor_window.clone();
        window.on_add_chain_input(move || {
            if let Some(cew) = chain_editor_window.borrow().as_ref() {
                cew.invoke_add_input();
            }
        });
    }
    {
        let chain_editor_window = chain_editor_window.clone();
        window.on_edit_chain_output(move |index| {
            if let Some(cew) = chain_editor_window.borrow().as_ref() {
                cew.invoke_edit_output(index);
            }
        });
    }
    {
        let chain_editor_window = chain_editor_window.clone();
        window.on_remove_chain_output(move |index| {
            if let Some(cew) = chain_editor_window.borrow().as_ref() {
                cew.invoke_remove_output(index);
            }
        });
    }
    {
        let chain_editor_window = chain_editor_window.clone();
        window.on_add_chain_output(move || {
            if let Some(cew) = chain_editor_window.borrow().as_ref() {
                cew.invoke_add_output();
            }
        });
    }
    {
        let chain_editor_window = chain_editor_window.clone();
        window.on_select_chain_instrument(move |index| {
            if let Some(cew) = chain_editor_window.borrow().as_ref() {
                cew.invoke_select_instrument(index);
            }
        });
    }
}
