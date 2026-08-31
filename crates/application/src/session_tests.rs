//! #913 — the session's sharing contract with the frontend.
//!
//! `from_shared` exists so the GUI's `ProjectSession` and the dispatcher work
//! on the SAME `Project` with no sync step: a mutation through either handle
//! must be visible through the other. `new` is the opposite — it owns a fresh
//! project nobody else holds.

use super::ApplicationSession;
use project::project::Project;
use std::cell::RefCell;
use std::rc::Rc;

fn empty_project(name: &str) -> Project {
    Project {
        name: Some(name.to_string()),
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    }
}

#[test]
fn a_shared_session_and_its_owner_see_the_same_mutation() {
    let shared = Rc::new(RefCell::new(empty_project("before")));
    let session = ApplicationSession::from_shared(Rc::clone(&shared));

    session.project.borrow_mut().name = Some("after".into());

    assert_eq!(
        shared.borrow().name.as_deref(),
        Some("after"),
        "the frontend's handle must see what the dispatcher wrote"
    );
    assert!(
        Rc::ptr_eq(&shared, &session.project),
        "from_shared must not clone the project"
    );
}

#[test]
fn a_mutation_through_the_owner_reaches_the_session() {
    let shared = Rc::new(RefCell::new(empty_project("before")));
    let session = ApplicationSession::from_shared(Rc::clone(&shared));
    shared.borrow_mut().chains.clear();
    shared.borrow_mut().name = Some("from the gui".into());
    assert_eq!(
        session.project.borrow().name.as_deref(),
        Some("from the gui")
    );
}

#[test]
fn a_new_session_owns_a_project_nobody_else_holds() {
    let session = ApplicationSession::new(empty_project("solo"));
    assert_eq!(Rc::strong_count(&session.project), 1);
    assert_eq!(session.project.borrow().name.as_deref(), Some("solo"));
}
