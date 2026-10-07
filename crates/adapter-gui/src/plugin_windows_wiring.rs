//! Responsibility: gives the library windows one path for their events to reach the UI.
//!
//! Their ticks drain the dispatcher's async results; what they drain reaches
//! the UI as the MCP drain's events do, so no event is lost.

use std::rc::Rc;

use application::event::Event;
use slint::ComponentHandle;

use crate::chain_rig_nav_wiring::{apply_events_to_ui, ChainRigNavCtx};
use crate::plugins_wiring::{wire_plugins, PluginWindows};
use crate::tone3000_ctx::ForwardEvents;
use crate::tone3000_session::SessionCell;
use crate::tone3000_wiring::wire_tone3000;
use crate::{AppWindow, Tone3000Window};

/// Wire the TONE3000 browser, the plugin catalog and the editor. Call once.
pub(crate) fn wire(
    window: &AppWindow,
    tone3000_window: &Tone3000Window,
    plugin_windows: PluginWindows<'_>,
    project_session: &SessionCell,
    nav_ctx: ChainRigNavCtx,
) {
    let weak_main = window.as_weak();
    let forward: ForwardEvents = Rc::new(move |events: &[Event]| {
        if let Some(w) = weak_main.upgrade() {
            apply_events_to_ui(&w, &nav_ctx, events);
        }
    });
    wire_tone3000(window, tone3000_window, project_session, forward.clone());
    wire_plugins(window, plugin_windows, project_session, forward);
}
