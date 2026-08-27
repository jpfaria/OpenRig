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
