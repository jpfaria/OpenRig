//! #913 — keeping the open draft in step with what is typed.
//!
//! Every keystroke reaches here. A draft that missed one saves the chain under
//! the previous name, and a callback that fires with no draft open (the editor
//! was closed under it) must not resurrect the abandoned edit.

use super::record_chain_name;
use crate::state::ChainDraft;
use std::cell::RefCell;
use std::rc::Rc;

fn draft() -> Rc<RefCell<Option<ChainDraft>>> {
    Rc::new(RefCell::new(Some(ChainDraft {
        editing_index: Some(0),
        name: "Guitar".into(),
        instrument: "electric_guitar".into(),
        io_binding_ids: vec![],
    })))
}

#[test]
fn the_open_draft_records_what_was_typed() {
    let draft = draft();
    assert!(record_chain_name(&draft, "Lead Guitar"));
    assert_eq!(draft.borrow().as_ref().unwrap().name, "Lead Guitar");
}

#[test]
fn clearing_the_field_records_the_empty_name() {
    let draft = draft();
    assert!(record_chain_name(&draft, ""));
    assert_eq!(draft.borrow().as_ref().unwrap().name, "");
}

#[test]
fn recording_a_name_leaves_the_rest_of_the_draft_alone() {
    let draft = draft();
    record_chain_name(&draft, "Lead");
    let borrowed = draft.borrow();
    let d = borrowed.as_ref().unwrap();
    assert_eq!(d.editing_index, Some(0));
    assert_eq!(d.instrument, "electric_guitar");
}

#[test]
fn with_no_draft_open_the_keystroke_is_dropped() {
    let none: Rc<RefCell<Option<ChainDraft>>> = Rc::new(RefCell::new(None));
    assert!(
        !record_chain_name(&none, "Lead"),
        "the editor was closed under the callback — do not resurrect the edit"
    );
    assert!(none.borrow().is_none());
}

#[test]
fn the_last_keystroke_wins() {
    let draft = draft();
    for typed in ["L", "Le", "Lea", "Lead"] {
        record_chain_name(&draft, typed);
    }
    assert_eq!(draft.borrow().as_ref().unwrap().name, "Lead");
}

// ── The instrument picker writes into the same draft ───────────────────────

use super::record_chain_instrument;

#[test]
fn the_open_draft_records_the_instrument_that_was_picked() {
    let draft = draft();
    assert!(record_chain_instrument(&draft, 1));
    let picked = draft.borrow().as_ref().unwrap().instrument.clone();
    assert!(!picked.is_empty());
    assert_eq!(
        picked,
        crate::chain_editor::instrument_index_to_string(1),
        "the index → id translation is shared with the detached editor"
    );
}

#[test]
fn picking_a_different_instrument_replaces_the_previous_one() {
    let draft = draft();
    record_chain_instrument(&draft, 0);
    let first = draft.borrow().as_ref().unwrap().instrument.clone();
    record_chain_instrument(&draft, 1);
    let second = draft.borrow().as_ref().unwrap().instrument.clone();
    assert_ne!(first, second);
}

#[test]
fn recording_an_instrument_leaves_the_name_alone() {
    let draft = draft();
    record_chain_instrument(&draft, 1);
    assert_eq!(draft.borrow().as_ref().unwrap().name, "Guitar");
}

#[test]
fn with_no_draft_open_the_instrument_pick_is_dropped() {
    let none: Rc<RefCell<Option<ChainDraft>>> = Rc::new(RefCell::new(None));
    assert!(!record_chain_instrument(&none, 1));
    assert!(none.borrow().is_none());
}

#[test]
fn an_index_outside_the_picker_still_records_a_usable_instrument() {
    // The translation is the single source of truth for what an index means;
    // whatever it answers for a stale index, the draft must never end up empty.
    let draft = draft();
    assert!(record_chain_instrument(&draft, 99));
    assert!(!draft.borrow().as_ref().unwrap().instrument.is_empty());
}
