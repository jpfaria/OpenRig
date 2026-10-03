//! Responsibility: keeps the drums panels in step with the dispatcher's state.
//!
//! The drums show in the standalone `DrumsWindow` (windowed desktop) and
//! inline over the chains page (fullscreen / touch); both read `DrumsBridge`,
//! so every redraw reaches the bridge of both surfaces.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::DrumsCommand;
use application::drums_state::DrumsSnapshot;
use application::live_source::LiveSource;
use slint::Global;

use crate::drums_bridge_sync::{set_drums_position, set_drums_view};
use crate::drums_session::{dispatch_drums, drums_panel_view, drums_snapshot, SessionCell};
use crate::{AppWindow, DrumsBridge, DrumsWindow};

#[derive(Clone)]
pub(crate) struct DrumsCtx {
    project_session: SessionCell,
    live: Rc<dyn LiveSource>,
    pub(crate) window: slint::Weak<DrumsWindow>,
    pub(crate) main_window: slint::Weak<AppWindow>,
    /// The state currently drawn; the poll redraws only when it differs.
    rendered: Rc<RefCell<Option<DrumsSnapshot>>>,
}

impl DrumsCtx {
    pub(crate) fn new(
        project_session: SessionCell,
        live: Rc<dyn LiveSource>,
        window: slint::Weak<DrumsWindow>,
        main_window: slint::Weak<AppWindow>,
    ) -> Self {
        Self {
            project_session,
            live,
            window,
            main_window,
            rendered: Rc::new(RefCell::new(None)),
        }
    }

    fn for_each_bridge(&self, mut f: impl FnMut(&DrumsBridge)) {
        if let Some(w) = self.window.upgrade() {
            f(&DrumsBridge::get(&w));
        }
        if let Some(w) = self.main_window.upgrade() {
            f(&DrumsBridge::get(&w));
        }
    }

    /// Draw the dispatcher's state; nothing without a project.
    pub(crate) fn render(&self) {
        let Some((snapshot, view)) = drums_panel_view(&self.project_session) else {
            return;
        };
        self.for_each_bridge(|bridge| set_drums_view(bridge, &view));
        *self.rendered.borrow_mut() = Some(snapshot);
    }

    /// Send one command and redraw when it was accepted.
    pub(crate) fn dispatch(&self, command: DrumsCommand) {
        if dispatch_drums(&self.project_session, command) {
            self.render();
        }
    }

    /// One frame: redraw a state changed from MCP or MIDI, move the lamps.
    pub(crate) fn tick(&self) {
        let current = drums_snapshot(&self.project_session);
        if current.is_some() && *self.rendered.borrow() != current {
            self.render();
        }
        let position = self.live.drums();
        self.for_each_bridge(|bridge| set_drums_position(bridge, position));
    }

    /// Drop the open picker and hide both surfaces.
    pub(crate) fn close(&self) {
        self.for_each_bridge(|bridge| bridge.set_picker(0));
        if let Some(w) = self.main_window.upgrade() {
            DrumsBridge::get(&w).set_show(false);
        }
        if let Some(w) = self.window.upgrade() {
            let _ = slint::ComponentHandle::hide(&w);
        }
    }
}

#[cfg(test)]
#[path = "drums_ctx_tests.rs"]
mod tests;
