//! #398: a scheme chosen over MCP/gRPC repaints the app like a Settings pick.

use std::cell::RefCell;
use std::rc::Rc;

use application::event::Event;
use infra_filesystem::Appearance;

use super::{apply, install};

#[test]
fn an_appearance_set_by_another_transport_reaches_the_windows() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    install(Rc::new(move |a: Appearance| sink.borrow_mut().push(a)));
    apply(&[
        Event::MidiLearnStarted,
        Event::AppearanceChanged {
            appearance: Appearance::Light,
        },
    ]);
    assert_eq!(*seen.borrow(), vec![Appearance::Light]);
}

#[test]
fn other_events_repaint_nothing() {
    let seen = Rc::new(RefCell::new(0));
    let sink = seen.clone();
    install(Rc::new(move |_: Appearance| *sink.borrow_mut() += 1));
    apply(&[Event::MidiLearnStopped]);
    assert_eq!(*seen.borrow(), 0);
}
