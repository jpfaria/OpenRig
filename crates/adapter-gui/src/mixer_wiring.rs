//! Responsibility: wires the global mixer window to the dispatcher.
//! #1007 — the mixer shows in two places, like the metronome: the standalone
//! `MixerWindow` (windowed desktop) and inline over the chains page
//! (fullscreen / touch). Both host `MixerPanel`, which reads `MixerBridge`, so
//! every write here reaches the bridge of BOTH surfaces.
//!
//! Every control dispatches a `Command::Mixer` and nothing else; the dispatcher
//! validates, applies and persists. While a surface is open a timer compares
//! the dispatcher's strips with what is drawn, so a fader moved over MCP or
//! MIDI follows on screen.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use application::command::{Command, MixerCommand};
use application::mixer_view::MixerStripView;
use slint::{ComponentHandle, Global, Timer, TimerMode};

use crate::helpers::{show_child_window, use_inline_block_editor};
use crate::mixer_rows::mixer_rows;
use crate::mixer_rows_sync::set_mixer_rows;
use crate::mixer_strip_intents::wire_strip_intents;
use crate::mixer_window_size::fit_mixer_window;
use crate::state::ProjectSession;
use crate::{AppWindow, MixerBridge, MixerWindow};

/// How often an open mixer re-reads the dispatcher. Slow enough to cost
/// nothing, fast enough that a surface fader feels live on screen.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

#[derive(Clone)]
struct MixerCtx {
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    timer: Rc<Timer>,
    window: slint::Weak<MixerWindow>,
    main_window: slint::Weak<AppWindow>,
    /// The strips currently drawn; the poll redraws only when they differ.
    rendered: Rc<RefCell<Option<Vec<MixerStripView>>>>,
    /// Strip count the standalone window was last sized for.
    fitted: Rc<Cell<Option<i32>>>,
}

impl MixerCtx {
    fn for_each_bridge(&self, mut f: impl FnMut(&MixerBridge)) {
        if let Some(w) = self.window.upgrade() {
            f(&MixerBridge::get(&w));
        }
        if let Some(w) = self.main_window.upgrade() {
            f(&MixerBridge::get(&w));
        }
    }

    fn strips(&self) -> Option<Vec<MixerStripView>> {
        self.project_session
            .borrow()
            .as_ref()
            .map(|session| session.dispatcher.mixer_strips())
    }

    fn render(&self) {
        let strips = self.strips().unwrap_or_default();
        let (inputs, outputs) = mixer_rows(&strips);
        self.for_each_bridge(|bridge| set_mixer_rows(bridge, inputs.clone(), outputs.clone()));
        *self.rendered.borrow_mut() = Some(strips);
    }

    fn dispatch(&self, command: MixerCommand) {
        {
            let borrowed = self.project_session.borrow();
            let Some(session) = borrowed.as_ref() else {
                return;
            };
            if let Err(e) = session.dispatcher.dispatch(Command::Mixer(command)) {
                log::warn!("[mixer] dispatch failed: {e}");
                return;
            }
        }
        self.render();
    }
}

/// Wire the mixer's open, close and strip callbacks. Call once per
/// `AppWindow + MixerWindow` pair.
pub(crate) fn wire_mixer(
    window: &AppWindow,
    mixer_window: &MixerWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
) {
    let ctx = MixerCtx {
        project_session: project_session.clone(),
        timer: Rc::new(Timer::default()),
        window: mixer_window.as_weak(),
        main_window: window.as_weak(),
        rendered: Rc::new(RefCell::new(None)),
        fitted: Rc::new(Cell::new(None)),
    };
    for bridge in [MixerBridge::get(window), MixerBridge::get(mixer_window)] {
        wire_open(&bridge, &ctx);
        wire_strip_controls(&bridge, &ctx);
        let close_ctx = ctx.clone();
        bridge.on_close_mixer(move || close(&close_ctx));
    }
    let close_ctx = ctx.clone();
    mixer_window.window().on_close_requested(move || {
        close(&close_ctx);
        slint::CloseRequestResponse::HideWindow
    });
}

fn wire_open(bridge: &MixerBridge, ctx: &MixerCtx) {
    let ctx = ctx.clone();
    bridge.on_open_mixer_window(move || {
        let Some(main_w) = ctx.main_window.upgrade() else {
            return;
        };
        ctx.render();
        start_poll(&ctx);
        if use_inline_block_editor(&main_w) {
            MixerBridge::get(&main_w).set_show(true);
        } else if let Some(mw) = ctx.window.upgrade() {
            ctx.fitted.set(None);
            fit_mixer_window(&mw, &ctx.fitted);
            show_child_window(main_w.window(), mw.window());
        }
    });
}

fn wire_strip_controls(bridge: &MixerBridge, ctx: &MixerCtx) {
    let c = ctx.clone();
    wire_strip_intents(bridge, Rc::new(move |command| c.dispatch(command)));
}

fn start_poll(ctx: &MixerCtx) {
    let poll_ctx = ctx.clone();
    ctx.timer
        .start(TimerMode::Repeated, POLL_INTERVAL, move || {
            let current = poll_ctx.strips();
            let changed = poll_ctx.rendered.borrow().as_ref() != current.as_ref();
            if changed {
                poll_ctx.render();
            }
            // A tab switch changes the strip count: re-fit the window to it.
            if let Some(mw) = poll_ctx.window.upgrade() {
                if mw.window().is_visible() {
                    fit_mixer_window(&mw, &poll_ctx.fitted);
                }
            }
        });
}

fn close(ctx: &MixerCtx) {
    if let Some(w) = ctx.main_window.upgrade() {
        MixerBridge::get(&w).set_show(false);
    }
    if let Some(w) = ctx.window.upgrade() {
        let _ = w.hide();
    }
    ctx.timer.stop();
}
