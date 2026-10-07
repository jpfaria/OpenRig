use std::cell::RefCell;
use std::rc::Rc;

use super::{is_editable, open, register, EditorLink};

#[test]
fn without_an_editor_nothing_is_editable() {
    // Each test runs on its own thread, so nothing is registered here.
    assert!(!is_editable("plexi"));
    open("plexi");
}

#[test]
fn the_registered_editor_answers_and_opens() {
    let opened = Rc::new(RefCell::new(vec![]));
    let o = opened.clone();
    register(EditorLink {
        editable: Rc::new(|id| id == "plexi"),
        open: Rc::new(move |id| o.borrow_mut().push(id.to_string())),
    });
    assert!(is_editable("plexi"));
    assert!(!is_editable("bundled"));
    open("plexi");
    assert_eq!(*opened.borrow(), vec!["plexi"]);
}
