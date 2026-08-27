//! Responsibility: records the chain name the user is typing.
//!
//! Split out of `chain_name_wiring` (#913). Mirroring the text into the window
//! property is screen logic; keeping the active draft in step with what was
//! typed is what has to happen — a draft that missed a keystroke saves the
//! chain under the previous name.

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

#[cfg(test)]
#[path = "chain_draft_name_tests.rs"]
mod tests;
