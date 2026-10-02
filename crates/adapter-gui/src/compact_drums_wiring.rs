//! Responsibility: keeps one compact chain view's DRUMS section live.
//!
//! The section lays the global drum machine flat: its controls dispatch the
//! same `DrumsCommand`s as the drums window, and a frame poll moves the lamps
//! and redraws when the state changes elsewhere, so every surface agrees.

use std::cell::RefCell;
use std::rc::Rc;

use application::drums_state::DrumsSnapshot;
use application::live_source::LiveSource;
use slint::{ComponentHandle, Global, Timer, TimerMode};

use crate::drums_bridge_sync::{set_drums_position, set_drums_view};
use crate::drums_intents::wire_drums_intents;
use crate::drums_session::{dispatch_drums, drums_panel_view, drums_snapshot, SessionCell};
use crate::{CompactChainViewWindow, DrumsBridge};

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(33);

#[derive(Clone)]
struct CompactDrumsCtx {
    project_session: SessionCell,
    window: slint::Weak<CompactChainViewWindow>,
    rendered: Rc<RefCell<Option<DrumsSnapshot>>>,
}

impl CompactDrumsCtx {
    fn render(&self) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let Some((snapshot, view)) = drums_panel_view(&self.project_session) else {
            return;
        };
        set_drums_view(&DrumsBridge::get(&w), &view);
        *self.rendered.borrow_mut() = Some(snapshot);
    }
}

/// Draw the drums in `compact_win` and keep them live while it exists.
pub(crate) fn wire(
    compact_win: &CompactChainViewWindow,
    project_session: &SessionCell,
    live: Rc<dyn LiveSource>,
) {
    let ctx = CompactDrumsCtx {
        project_session: project_session.clone(),
        window: compact_win.as_weak(),
        rendered: Rc::new(RefCell::new(None)),
    };
    let bridge = DrumsBridge::get(compact_win);
    let c = ctx.clone();
    wire_drums_intents(
        &bridge,
        Rc::new(move |command| {
            if dispatch_drums(&c.project_session, command) {
                c.render();
            }
        }),
    );
    let c = ctx.clone();
    bridge.on_output_opened(move || c.render());
    ctx.render();
    start_poll(ctx, live);
}

fn start_poll(ctx: CompactDrumsCtx, live: Rc<dyn LiveSource>) {
    let timer = Rc::new(Timer::default());
    let own = Rc::downgrade(&timer);
    timer.start(TimerMode::Repeated, POLL_INTERVAL, move || {
        // The window is gone: this poll has nothing left to feed.
        let Some(w) = ctx.window.upgrade() else {
            if let Some(t) = own.upgrade() {
                t.stop();
            }
            return;
        };
        if !w.window().is_visible() {
            return;
        }
        let current = drums_snapshot(&ctx.project_session);
        if current.is_some() && *ctx.rendered.borrow() != current {
            ctx.render();
        }
        set_drums_position(&DrumsBridge::get(&w), live.drums());
    });
    // The timer lives as long as its window; it stops itself above.
    std::mem::forget(timer);
}
