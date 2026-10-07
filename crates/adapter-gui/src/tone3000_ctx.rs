//! Responsibility: keeps the TONE3000 window in step with the dispatcher's state.
//!
//! A search or an install answers off-thread. While one is in flight the tick
//! drains the dispatcher's async results itself, so the window updates
//! without the MCP drain running, and hands every drained event on: the
//! drain is shared by every domain, and an event it swallows is lost (#879).

use std::cell::RefCell;
use std::rc::Rc;

use application::command::Tone3000Command;
use application::event::Event;
use application::tone3000::Tone3000Architecture;
use application::tone3000_state::Tone3000Snapshot;
use slint::Global;

use crate::tone3000_bridge_sync::set_tone3000_view;
use crate::tone3000_session::{dispatch_tone3000, poll_tone3000, tone3000_snapshot, SessionCell};
use crate::tone3000_view::{tone3000_view, ArchChoices};
use crate::{Tone3000Bridge, Tone3000Window};

/// Where the events a tick drained go next.
pub(crate) type ForwardEvents = Rc<dyn Fn(&[Event])>;

#[derive(Clone)]
pub(crate) struct Tone3000Ctx {
    project_session: SessionCell,
    pub(crate) window: slint::Weak<Tone3000Window>,
    choices: Rc<RefCell<ArchChoices>>,
    /// Narrows the Installed tab by name.
    installed_query: Rc<RefCell<String>>,
    /// The state currently drawn; the tick redraws only when it differs.
    rendered: Rc<RefCell<Option<Tone3000Snapshot>>>,
    /// Why the last command was refused; cleared by the next accepted one.
    refusal: Rc<RefCell<String>>,
    forward: ForwardEvents,
}

impl Tone3000Ctx {
    pub(crate) fn new(
        project_session: SessionCell,
        window: slint::Weak<Tone3000Window>,
        forward: ForwardEvents,
    ) -> Self {
        Self {
            project_session,
            window,
            choices: Rc::new(RefCell::new(ArchChoices::new())),
            installed_query: Rc::new(RefCell::new(String::new())),
            rendered: Rc::new(RefCell::new(None)),
            refusal: Rc::new(RefCell::new(String::new())),
            forward,
        }
    }

    /// Draw the dispatcher's state; nothing without a project.
    pub(crate) fn render(&self) {
        let Some(snapshot) = tone3000_snapshot(&self.project_session) else {
            return;
        };
        let mut view = tone3000_view(
            &snapshot,
            &self.choices.borrow(),
            &self.installed_query.borrow(),
        );
        if view.error.is_empty() {
            view.error = self.refusal.borrow().clone();
        }
        if let Some(w) = self.window.upgrade() {
            set_tone3000_view(&Tone3000Bridge::get(&w), &view);
        }
        *self.rendered.borrow_mut() = Some(snapshot);
    }

    /// Send one command and redraw, showing why when it was refused.
    pub(crate) fn dispatch(&self, command: Tone3000Command) {
        *self.refusal.borrow_mut() = match dispatch_tone3000(&self.project_session, command) {
            Ok(()) => String::new(),
            Err(why) => why,
        };
        self.render();
    }

    /// Remember the architecture the user picked for a tone.
    pub(crate) fn pick(&self, tone_id: u64, arch: Tone3000Architecture) {
        self.choices.borrow_mut().insert(tone_id, arch);
        self.render();
    }

    /// Narrow the Installed tab to names holding `query`.
    pub(crate) fn filter_installed(&self, query: &str) {
        *self.installed_query.borrow_mut() = query.to_string();
        self.render();
    }

    #[cfg(test)]
    pub(crate) fn installed_query(&self) -> String {
        self.installed_query.borrow().clone()
    }

    pub(crate) fn arch_of(&self, tone_id: u64) -> Option<Tone3000Architecture> {
        self.choices.borrow().get(&tone_id).copied()
    }

    /// One tick: collect what finished off-thread, redraw a changed state.
    pub(crate) fn tick(&self) {
        if self.rendered.borrow().as_ref().is_some_and(in_flight) {
            let events = poll_tone3000(&self.project_session);
            if !events.is_empty() {
                (self.forward)(&events);
            }
        }
        let current = tone3000_snapshot(&self.project_session);
        if current.is_some() && *self.rendered.borrow() != current {
            self.render();
        }
    }
}

/// A search or an install is still waiting for its answer.
fn in_flight(snapshot: &Tone3000Snapshot) -> bool {
    snapshot.search.in_flight || snapshot.installs.iter().any(|i| i.error.is_none())
}

#[cfg(test)]
#[path = "tone3000_ctx_tests.rs"]
mod tests;
