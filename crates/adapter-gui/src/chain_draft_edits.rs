//! Responsibility: records what the chain editor typed into the open draft.
//!
//! Split out of `chain_name_wiring` and `chain_editor_meta_io_callbacks`
//! (#913). Mirroring the values into the window properties is screen logic;
//! keeping the active draft in step with the fields is what has to happen — a
//! draft that missed a keystroke saves the chain under the previous name, and
//! one that missed the instrument saves it as the previous instrument.

use std::cell::RefCell;
use std::rc::Rc;

use crate::state::ChainDraft;

/// Record `value` as the draft's name. No draft open ⇒ nothing to record: the
/// callback can fire from a window whose editor was already closed, and
/// inventing a draft there would resurrect an edit the user abandoned.
///
/// Returns whether a draft took the name, so the caller knows whether the
/// window property is worth updating.
pub(crate) fn record_chain_name(draft: &Rc<RefCell<Option<ChainDraft>>>, value: &str) -> bool {
    match draft.borrow_mut().as_mut() {
        Some(draft) => {
            draft.name = value.to_string();
            true
        }
        None => false,
    }
}

/// Record the instrument at picker `index` on the draft. Same contract as
/// [`record_chain_name`]: no draft open ⇒ nothing to record.
///
/// The index → id translation is `chain_editor`'s, shared with the detached editor
/// window, so the two surfaces cannot disagree about what index 2 means.
pub(crate) fn record_chain_instrument(draft: &Rc<RefCell<Option<ChainDraft>>>, index: i32) -> bool {
    let instrument = crate::chain_editor::instrument_index_to_string(index).to_string();
    match draft.borrow_mut().as_mut() {
        Some(draft) => {
            draft.instrument = instrument;
            true
        }
        None => {
            log::warn!("[select_instrument] no draft to update");
            false
        }
    }
}

#[cfg(test)]
#[path = "chain_draft_edits_tests.rs"]
mod tests;
