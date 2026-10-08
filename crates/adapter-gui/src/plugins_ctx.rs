//! Responsibility: keeps the plugin catalog window in step with the user's plugins.
//!
//! A save, a restore and an uninstall answer at once. A redo of a TONE3000
//! plugin talks to the network, so its answer arrives on a later drain: the
//! row stays busy until `on_events` sees it, and while one runs the tick
//! drains the dispatcher itself and hands every event on.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use application::command::{Command, PluginLibraryCommand};
use application::event::{Event, PluginLibraryEvent};
use slint::Global;

use crate::plugin_library_session::{dispatch_library, plugin_entries};
use crate::plugins_bridge_sync::{set_plugins_view, PluginsChrome};
use crate::plugins_view::{plugins_view, PluginsFilter};
use crate::tone3000_ctx::ForwardEvents;
use crate::tone3000_session::{poll_tone3000, SessionCell};
use crate::{PluginsBridge, PluginsWindow};

#[derive(Clone)]
pub(crate) struct PluginsCtx {
    project_session: SessionCell,
    pub(crate) window: slint::Weak<PluginsWindow>,
    filter: Rc<RefCell<PluginsFilter>>,
    type_query: Rc<RefCell<String>>,
    /// The plugins whose redo is running.
    busy: Rc<RefCell<BTreeSet<String>>>,
    /// Why the last command failed; cleared by the next accepted one.
    error: Rc<RefCell<String>>,
    forward: ForwardEvents,
}

impl PluginsCtx {
    pub(crate) fn new(
        project_session: SessionCell,
        window: slint::Weak<PluginsWindow>,
        forward: ForwardEvents,
    ) -> Self {
        Self {
            project_session,
            window,
            filter: Rc::new(RefCell::new(PluginsFilter::default())),
            type_query: Rc::new(RefCell::new(String::new())),
            busy: Rc::new(RefCell::new(BTreeSet::new())),
            error: Rc::new(RefCell::new(String::new())),
            forward,
        }
    }

    /// Draw the user's plugins through the filters; nothing without a project.
    pub(crate) fn render(&self) {
        let Some(entries) = plugin_entries(&self.project_session) else {
            return;
        };
        let view = plugins_view(&entries, &self.filter.borrow());
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let busy = self.busy.borrow();
        let error = self.error.borrow();
        let type_query = self.type_query.borrow();
        let chrome = PluginsChrome {
            busy: &busy,
            error: &error,
            type_query: &type_query,
        };
        set_plugins_view(&PluginsBridge::get(&w), &view, &chrome);
    }

    pub(crate) fn set_filter(&self, filter: PluginsFilter) {
        *self.filter.borrow_mut() = filter;
        self.render();
    }

    /// Narrow the type filter's choices to the types holding `query`.
    pub(crate) fn set_type_query(&self, query: &str) {
        *self.type_query.borrow_mut() = query.to_string();
        self.render();
    }

    /// Show why something the window asked for failed.
    pub(crate) fn show_error(&self, why: String) {
        *self.error.borrow_mut() = why;
        self.render();
    }

    /// Send one command and redraw; `false` when it was refused.
    pub(crate) fn dispatch(&self, command: Command) -> bool {
        match dispatch_library(&self.project_session, command) {
            Ok(events) => {
                self.error.borrow_mut().clear();
                self.absorb(&events);
                self.render();
                if !events.is_empty() {
                    (self.forward)(&events);
                }
                true
            }
            Err(why) => {
                self.show_error(why);
                false
            }
        }
    }

    pub(crate) fn uninstall(&self, plugin_id: &str) {
        self.dispatch(Command::PluginLibrary(
            PluginLibraryCommand::UninstallPlugin {
                plugin_id: plugin_id.to_string(),
            },
        ));
    }

    /// Build the parameters again; the row stays busy until the answer.
    pub(crate) fn redo(&self, plugin_id: &str) {
        self.busy.borrow_mut().insert(plugin_id.to_string());
        let accepted = self.dispatch(Command::PluginLibrary(
            PluginLibraryCommand::RedoPluginParameters {
                plugin_id: plugin_id.to_string(),
            },
        ));
        if !accepted {
            self.busy.borrow_mut().remove(plugin_id);
            self.render();
        }
    }

    /// A drain delivered library events: settle the redos and redraw.
    pub(crate) fn on_events(&self, events: &[Event]) {
        self.absorb(events);
        self.render();
    }

    /// One tick: while a redo runs, collect what finished off-thread.
    pub(crate) fn tick(&self) {
        if self.busy.borrow().is_empty() {
            return;
        }
        let events = poll_tone3000(&self.project_session);
        if !events.is_empty() {
            (self.forward)(&events);
        }
    }

    #[cfg(test)]
    pub(crate) fn is_busy(&self, plugin_id: &str) -> bool {
        self.busy.borrow().contains(plugin_id)
    }

    #[cfg(test)]
    pub(crate) fn filter(&self) -> PluginsFilter {
        self.filter.borrow().clone()
    }

    fn absorb(&self, events: &[Event]) {
        for event in events {
            match event {
                Event::PluginLibrary(PluginLibraryEvent::Redone { plugin_id, .. }) => {
                    self.busy.borrow_mut().remove(plugin_id);
                }
                Event::PluginLibrary(PluginLibraryEvent::RedoFailed { plugin_id, message }) => {
                    self.busy.borrow_mut().remove(plugin_id);
                    *self.error.borrow_mut() = message.clone();
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
#[path = "plugins_ctx_tests.rs"]
mod tests;
