//! Responsibility: wires the global drum machine surfaces to the dispatcher.
//!
//! The drums show like the mixer: the standalone `DrumsWindow` (windowed
//! desktop) and inline over the chains page (fullscreen / touch). Both host
//! `DrumsPanel`, which reads `DrumsBridge`, so every write reaches the bridge
//! of both surfaces. A frame poll keeps the beat lamps moving and redraws the
//! panel when the dispatcher's state changes from MCP or MIDI; it also keeps
//! the top-bar icon lit while a groove plays with the panel closed.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::DrumsCommand;
use application::drums_state::DrumsSnapshot;
use application::live_source::LiveSource;
use slint::{ComponentHandle, Global, Timer, TimerMode};

use crate::drums_bridge_sync::{set_drums_position, set_drums_view};
use crate::drums_intents::wire_drums_intents;
use crate::drums_session::{dispatch_drums, drums_panel_view, drums_snapshot, SessionCell};
use crate::helpers::{show_child_window, use_inline_block_editor};
use crate::{AppWindow, DrumsBridge, DrumsWindow};

/// One frame: the lamps follow the groove without visible lag.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(33);

#[derive(Clone)]
struct DrumsCtx {
    project_session: SessionCell,
    live: Rc<dyn LiveSource>,
    timer: Rc<Timer>,
    window: slint::Weak<DrumsWindow>,
    main_window: slint::Weak<AppWindow>,
    /// The state currently drawn; the poll redraws only when it differs.
    rendered: Rc<RefCell<Option<DrumsSnapshot>>>,
}

impl DrumsCtx {
    fn for_each_bridge(&self, mut f: impl FnMut(&DrumsBridge)) {
        if let Some(w) = self.window.upgrade() {
            f(&DrumsBridge::get(&w));
        }
        if let Some(w) = self.main_window.upgrade() {
            f(&DrumsBridge::get(&w));
        }
    }

    fn render(&self) {
        let Some((snapshot, view)) = drums_panel_view(&self.project_session) else {
            return;
        };
        self.for_each_bridge(|bridge| set_drums_view(bridge, &view));
        *self.rendered.borrow_mut() = Some(snapshot);
    }

    fn dispatch(&self, command: DrumsCommand) {
        if dispatch_drums(&self.project_session, command) {
            self.render();
        }
    }

    fn tick(&self) {
        let current = drums_snapshot(&self.project_session);
        if current.is_some() && *self.rendered.borrow() != current {
            self.render();
        }
        let position = self.live.drums();
        self.for_each_bridge(|bridge| set_drums_position(bridge, position));
    }
}

/// Wire the drums' open, close and control callbacks. Call once per
/// `AppWindow + DrumsWindow` pair.
pub(crate) fn wire_drums(
    window: &AppWindow,
    drums_window: &DrumsWindow,
    project_session: &SessionCell,
    live: Rc<dyn LiveSource>,
) {
    let ctx = DrumsCtx {
        project_session: project_session.clone(),
        live,
        timer: Rc::new(Timer::default()),
        window: drums_window.as_weak(),
        main_window: window.as_weak(),
        rendered: Rc::new(RefCell::new(None)),
    };
    for bridge in [DrumsBridge::get(window), DrumsBridge::get(drums_window)] {
        let c = ctx.clone();
        wire_drums_intents(&bridge, Rc::new(move |command| c.dispatch(command)));
        let c = ctx.clone();
        bridge.on_output_opened(move || c.render());
        let c = ctx.clone();
        bridge.on_query_changed(move |_| c.render());
        let c = ctx.clone();
        bridge.on_open_drums_window(move || open(&c));
        let c = ctx.clone();
        bridge.on_close_drums(move || close(&c));
    }
    let c = ctx.clone();
    drums_window.window().on_close_requested(move || {
        close(&c);
        slint::CloseRequestResponse::HideWindow
    });
    let c = ctx.clone();
    ctx.timer
        .start(TimerMode::Repeated, POLL_INTERVAL, move || c.tick());
}

fn open(ctx: &DrumsCtx) {
    let Some(main_w) = ctx.main_window.upgrade() else {
        return;
    };
    ctx.render();
    if use_inline_block_editor(&main_w) {
        DrumsBridge::get(&main_w).set_show(true);
    } else if let Some(dw) = ctx.window.upgrade() {
        show_child_window(main_w.window(), dw.window());
    }
}

fn close(ctx: &DrumsCtx) {
    ctx.for_each_bridge(|bridge| bridge.set_picker(0));
    if let Some(w) = ctx.main_window.upgrade() {
        DrumsBridge::get(&w).set_show(false);
    }
    if let Some(w) = ctx.window.upgrade() {
        let _ = w.hide();
    }
}
