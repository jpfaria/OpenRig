//! Wiring for the chain Input/Output picker windows' simple callbacks.
//!
//! Owns the 5 callbacks shared by `ChainInputWindow` and `ChainOutputWindow`:
//! select-device and toggle-channel (both forwarders to the main window's
//! `invoke_*` methods) plus the output-mode selector that mutates the active
//! `ChainDraft`.
//!
//! The bigger save/cancel/select-input-mode callbacks live elsewhere and will
//! move in dedicated slices (they capture broader project state).

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;

use crate::chain_editor::{input_mode_from_index, output_mode_from_index};
use crate::state::ChainDraft;
use crate::{AppWindow, ChainInputWindow, ChainOutputWindow};

pub(crate) struct ChainIoPickerCtx {
    pub chain_draft: Rc<RefCell<Option<ChainDraft>>>,
}

pub(crate) fn wire(
    window: &AppWindow,
    chain_input_window: &ChainInputWindow,
    chain_output_window: &ChainOutputWindow,
    ctx: ChainIoPickerCtx,
) {
    let ChainIoPickerCtx { chain_draft } = ctx;

    {
        let weak_window = window.as_weak();
        chain_input_window.on_select_device(move |index| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_select_chain_input_device(index);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        chain_input_window.on_toggle_channel(move |index, selected| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_toggle_chain_input_channel(index, selected);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        chain_output_window.on_select_device(move |index| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_select_chain_output_device(index);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        chain_output_window.on_toggle_channel(move |index, selected| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_toggle_chain_output_channel(index, selected);
            }
        });
    }
    {
        let chain_draft = chain_draft.clone();
        chain_input_window.on_select_input_mode(move |index| {
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                if let Some(gi) = draft.editing_input_index {
                    if let Some(input) = draft.inputs.get_mut(gi) {
                        input.mode = input_mode_from_index(index);
                        log::debug!(
                            "[select_input_mode] group={}, index={}, mode={:?}",
                            gi,
                            index,
                            input.mode
                        );
                    }
                }
            }
        });
    }
    {
        let chain_draft = chain_draft.clone();
        chain_output_window.on_select_output_mode(move |index| {
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                if let Some(gi) = draft.editing_output_index {
                    if let Some(output) = draft.outputs.get_mut(gi) {
                        output.mode = output_mode_from_index(index);
                        log::debug!(
                            "[select_output_mode] group={}, index={}, mode={:?}",
                            gi,
                            index,
                            output.mode
                        );
                    }
                }
            }
        });
    }
}
