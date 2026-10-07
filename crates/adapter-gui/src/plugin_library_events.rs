//! Responsibility: delivers plugin library events to the windows that wait for them.
//!
//! A redo and a create finish off-thread, so their result comes back on
//! whichever drain runs first. Every drain hands its events to
//! `apply_events_to_ui`, which calls [`apply`]; the plugin windows register
//! their listener here once.

use std::cell::RefCell;
use std::rc::Rc;

use application::event::Event;

type Listener = Rc<dyn Fn(&[Event])>;

thread_local! {
    static LISTENER: RefCell<Option<Listener>> = const { RefCell::new(None) };
}

pub(crate) fn register(listener: Listener) {
    LISTENER.with(|l| *l.borrow_mut() = Some(listener));
}

/// Hand `events` to the registered listener, if any holds a library event.
pub(crate) fn apply(events: &[Event]) {
    let relevant = events
        .iter()
        .any(|e| matches!(e, Event::PluginLibrary(_) | Event::Tone3000(_)));
    if !relevant {
        return;
    }
    let listener = LISTENER.with(|l| l.borrow().clone());
    if let Some(listener) = listener {
        listener(events);
    }
}
