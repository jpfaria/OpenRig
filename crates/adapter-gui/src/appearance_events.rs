//! Responsibility: repaints the scheme another transport chose.
//!
//! #398: `SetAppearance` sent over MCP/gRPC lands as
//! `Event::AppearanceChanged`; the Settings section installs the same repaint
//! a pick in the app runs, and the GUI's event drain hands every event here.

use std::cell::RefCell;
use std::rc::Rc;

use application::event::Event;
use infra_filesystem::Appearance;

thread_local! {
    static REFLECT: RefCell<Option<Rc<dyn Fn(Appearance)>>> = const { RefCell::new(None) };
}

pub(crate) fn install(reflect: Rc<dyn Fn(Appearance)>) {
    REFLECT.with(|r| *r.borrow_mut() = Some(reflect));
}

pub(crate) fn apply(events: &[Event]) {
    let Some(reflect) = REFLECT.with(|r| r.borrow().clone()) else {
        return;
    };
    for event in events {
        if let Event::AppearanceChanged { appearance } = event {
            reflect(*appearance);
        }
    }
}

#[cfg(test)]
#[path = "appearance_events_tests.rs"]
mod tests;
