//! Responsibility: registers the drum machine's window callbacks.
//!
//! Everything the callbacks do lives in `DrumsCtx` (`drums_ctx.rs`), which is
//! tested; this file only hands it the windows, the intents and the frame
//! poll that keeps the beat lamps moving and the top-bar icon lit while a
//! groove plays with the panel closed.

use std::rc::Rc;

use application::live_source::LiveSource;
use slint::{ComponentHandle, Global, Timer, TimerMode};

use crate::drums_ctx::DrumsCtx;
use crate::drums_intents::wire_drums_intents;
use crate::drums_session::SessionCell;
use crate::helpers::{show_child_window, use_inline_block_editor};
use crate::{AppWindow, DrumsBridge, DrumsWindow};

/// One frame: the lamps follow the groove without visible lag.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(33);

/// Wire the drums' open, close and control callbacks. Call once per
/// `AppWindow + DrumsWindow` pair.
pub(crate) fn wire_drums(
    window: &AppWindow,
    drums_window: &DrumsWindow,
    project_session: &SessionCell,
    live: Rc<dyn LiveSource>,
) {
    let ctx = DrumsCtx::new(
        project_session.clone(),
        live,
        drums_window.as_weak(),
        window.as_weak(),
    );
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
        bridge.on_close_drums(move || c.close());
    }
    let c = ctx.clone();
    drums_window.window().on_close_requested(move || {
        c.close();
        slint::CloseRequestResponse::HideWindow
    });
    // The closure owns its timer, so the poll lives as long as the app.
    let timer = Rc::new(Timer::default());
    let owned = timer.clone();
    timer.start(TimerMode::Repeated, POLL_INTERVAL, move || {
        let _ = &owned;
        ctx.tick();
    });
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
