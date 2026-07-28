//! MetronomeWindow wiring (#14) — the single entry point plus the open, close
//! and power paths for the top-bar metronome. Mirrors `tuner_wiring.rs`.
//!
//! The metronome shows in two places — the standalone `MetronomeWindow`
//! (windowed desktop) and inline over the chains page (fullscreen / touch) —
//! and both host the same `MetronomePanel`, which reads its state from the
//! `MetronomeBridge` global. So every state write and every callback here goes
//! through the bridge of BOTH live surfaces via [`MetronomeCtx::for_each_bridge`]
//! rather than a single window's properties.
//!
//! Two rules shape this file and its siblings:
//!
//! * **Every control goes through the dispatcher.** A knob callback dispatches
//!   its `Command` and reacts to the `Event` that comes back — it never edits
//!   the session or the audio state directly. That keeps MCP, MIDI and the GUI
//!   on one door, and it is why `metronome_events::apply_events` is the only
//!   place that writes to the session, the UI, `config.yaml` and the runtime.
//! * **The lamps read a phase, not a queue.** The timer samples
//!   `MetronomeShared::position()`, so a frame that arrives late shows the beat
//!   the click is actually on instead of replaying a backlog.

use std::cell::RefCell;
use std::rc::Rc;

use infra_cpal::ProjectRuntimeController;
use slint::{ComponentHandle, Global, Timer, TimerMode};

use crate::helpers::{show_child_window, use_inline_block_editor};
use crate::metronome_controls_wiring::{
    refresh_metronome_outputs, wire_controls, wire_output_select,
};
use crate::metronome_events::{dispatch, render_settings};
use crate::metronome_session::{resolve_output_endpoint, MetronomeOutput, MetronomeSession};
use crate::state::ProjectSession;
use crate::{AppWindow, MetronomeBridge, MetronomeWindow};

use application::command::{Command, MetronomeCommand};

/// Beat-lamp refresh. Fast enough that the lamp lands with the click at any
/// tempo the generator supports, cheap enough to be a single atomic load.
const TICK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(33);

/// Everything the metronome callbacks need to reach. Grouped because there are
/// eleven callbacks and they all want the same handful of things.
pub(crate) struct MetronomeCtx {
    pub(crate) project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub(crate) project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub(crate) session: Rc<RefCell<MetronomeSession>>,
    pub(crate) timer: Rc<Timer>,
    /// The standalone window (windowed desktop mode).
    pub(crate) window: slint::Weak<MetronomeWindow>,
    /// The main window, whose bridge drives the inline panel (fullscreen/touch).
    pub(crate) main_window: slint::Weak<AppWindow>,
    /// The project's output endpoints as published to the select, cached so
    /// each keystroke filters the list instead of re-reading the bindings.
    pub(crate) outputs: Rc<RefCell<Vec<MetronomeOutput>>>,
}

impl MetronomeCtx {
    pub(crate) fn clone_ctx(&self) -> Self {
        Self {
            project_session: self.project_session.clone(),
            project_runtime: self.project_runtime.clone(),
            session: self.session.clone(),
            timer: self.timer.clone(),
            window: self.window.clone(),
            main_window: self.main_window.clone(),
            outputs: self.outputs.clone(),
        }
    }

    /// Run `f` against the `MetronomeBridge` of every live surface — the
    /// standalone window and the inline panel's main window. Both host the same
    /// panel reading the same global, so a state write has to reach both.
    pub(crate) fn for_each_bridge(&self, mut f: impl FnMut(&MetronomeBridge)) {
        if let Some(mw) = self.window.upgrade() {
            f(&MetronomeBridge::get(&mw));
        }
        if let Some(aw) = self.main_window.upgrade() {
            f(&MetronomeBridge::get(&aw));
        }
    }
}

/// Wire every metronome callback (open / close / power / controls) onto the
/// supplied windows. Call once per `AppWindow + MetronomeWindow` pair.
pub fn wire_metronome(
    window: &AppWindow,
    metronome_window: &MetronomeWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    metronome_session: &Rc<RefCell<MetronomeSession>>,
    metronome_timer: &Rc<Timer>,
) {
    let ctx = MetronomeCtx {
        project_session: project_session.clone(),
        project_runtime: project_runtime.clone(),
        session: metronome_session.clone(),
        timer: metronome_timer.clone(),
        window: metronome_window.as_weak(),
        main_window: window.as_weak(),
        outputs: Rc::new(RefCell::new(Vec::new())),
    };

    wire_open(window, metronome_window, &ctx);
    wire_close(window, metronome_window, &ctx);

    // Controls and power live on the bridge, so connect them on BOTH surfaces'
    // bridges — whichever one the user is looking at fires the same dispatch.
    for bridge in [
        MetronomeBridge::get(metronome_window),
        MetronomeBridge::get(window),
    ] {
        wire_power(&bridge, &ctx);
        wire_controls(&bridge, &ctx);
        wire_output_select(&bridge, &ctx);
    }
}

// ── open / close ────────────────────────────────────────────────────────

fn wire_open(window: &AppWindow, metronome_window: &MetronomeWindow, ctx: &MetronomeCtx) {
    let metronome_window_weak = metronome_window.as_weak();
    let main_window_weak = window.as_weak();
    let ctx = ctx.clone_ctx();
    window.on_open_metronome_window(move || {
        let Some(main_w) = main_window_weak.upgrade() else {
            return;
        };
        // Show the CURRENT state: the persisted settings, and POWER reflecting
        // whether the click is actually playing — closing the window only hides
        // it (the click keeps going), so reopening must not look stopped.
        render_settings(&ctx);
        let playing = ctx
            .project_runtime
            .borrow()
            .as_ref()
            .is_some_and(|rt| rt.metronome_shared().enabled());
        set_power_display(&ctx, playing);

        if use_inline_block_editor(&main_w) {
            // Fullscreen / touch: the inline panel is gated by the global.
            MetronomeBridge::get(&main_w).set_show(true);
        } else if let Some(mw) = metronome_window_weak.upgrade() {
            show_child_window(main_w.window(), mw.window());
        }
    });
}

fn wire_close(window: &AppWindow, metronome_window: &MetronomeWindow, ctx: &MetronomeCtx) {
    // The panel's close button lives on the bridge; the standalone window can
    // also be closed via the OS chrome. Wire BOTH so either path hides the
    // surface — the click keeps playing regardless (only POWER stops it).
    for bridge in [
        MetronomeBridge::get(metronome_window),
        MetronomeBridge::get(window),
    ] {
        let ctx = ctx.clone_ctx();
        bridge.on_close_metronome(move || close_metronome(&ctx));
    }
    {
        let ctx = ctx.clone_ctx();
        metronome_window.window().on_close_requested(move || {
            close_metronome(&ctx);
            slint::CloseRequestResponse::HideWindow
        });
    }
}

/// Closing the metronome only HIDES it — the click keeps playing, and only
/// POWER stops it. So a player can start the click, close the window, and keep
/// working in the chains screen with the tempo still going.
fn close_metronome(ctx: &MetronomeCtx) {
    // Hide whichever surface was showing: the inline panel (clear `show`) and
    // the standalone window (hide it). Do NOT stop the click or the timer.
    if let Some(aw) = ctx.main_window.upgrade() {
        MetronomeBridge::get(&aw).set_show(false);
    }
    if let Some(mw) = ctx.window.upgrade() {
        let _ = mw.hide();
    }
}

/// Reflect the power state (and reset the lamp) on every surface's bridge.
pub(crate) fn set_power_display(ctx: &MetronomeCtx, enabled: bool) {
    ctx.for_each_bridge(|bridge| {
        bridge.set_metronome_enabled(enabled);
        if !enabled {
            bridge.set_current_beat(0);
            bridge.set_counting_in(false);
        }
    });
}

// ── power ───────────────────────────────────────────────────────────────

fn wire_power(bridge: &MetronomeBridge, ctx: &MetronomeCtx) {
    let ctx = ctx.clone_ctx();
    bridge.on_toggle_enabled(move |enabled| {
        dispatch(
            &ctx,
            Command::Metronome(MetronomeCommand::SetMetronomeEnabled { enabled }),
        );
        // With no project open there is no dispatcher and therefore no event —
        // reflect the request on the switch anyway so it never looks stuck.
        set_power_display(&ctx, enabled);
    });
}

/// The metronome is an independent pipeline (invariant #4) — it must open with
/// NO chain enabled. The runtime controller is created lazily on chain-enable
/// (#808), so create it here if it does not exist yet; otherwise pressing POWER
/// with nothing enabled opens no stream and the click never plays.
pub(crate) fn ensure_metronome_runtime(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
) {
    let session_borrow = project_session.borrow();
    let Some(session) = session_borrow.as_ref() else {
        return; // launcher / no project open — nothing to build a runtime from
    };
    if let Err(e) = crate::runtime_lifecycle::ensure_runtime(project_runtime, session) {
        log::error!("[metronome] ensure_runtime failed: {e}");
    }
}

/// Open the metronome's own output stream and start the lamp timer.
pub(crate) fn start_click(ctx: &MetronomeCtx) {
    let settings = ctx.session.borrow().settings();
    let saved = ctx.session.borrow().output_device().map(str::to_string);
    // The metronome plays through one of the project's configured output
    // endpoints (#14) — same device and channels the guitar's output uses.
    let outputs = refresh_metronome_outputs(&ctx.project_session, &ctx.outputs);
    let target = resolve_output_endpoint(saved.as_deref(), &outputs);
    // #14: the click plays even with no chain enabled (invariant #4). The
    // runtime is created lazily on chain-enable, so make sure it exists.
    ensure_metronome_runtime(&ctx.project_runtime, &ctx.project_session);
    if let Some(rt) = ctx.project_runtime.borrow().as_ref() {
        rt.set_metronome_settings(settings);
        let shared = rt.metronome_shared();
        shared.set_enabled(true);
        // Start from beat one of the bar instead of wherever a previous run
        // left the phase.
        shared.request_restart();
        match target {
            Some(out) => {
                if let Err(e) = rt.start_metronome(&out.device_id, &out.channels) {
                    log::warn!("[metronome] start on '{}' failed: {e}", out.label);
                }
            }
            None => log::warn!("[metronome] no project output endpoint to play through"),
        }
    }
    start_lamp_timer(ctx);
}

pub(crate) fn stop_click(ctx: &MetronomeCtx) {
    ctx.timer.stop();
    if let Some(rt) = ctx.project_runtime.borrow().as_ref() {
        rt.metronome_shared().set_enabled(false);
        rt.stop_metronome();
    }
}

/// Sample the generator's position onto the beat lamps. Reading the phase (not
/// a queue of beat events) is what makes a late frame harmless.
fn start_lamp_timer(ctx: &MetronomeCtx) {
    let ctx = ctx.clone_ctx();
    ctx.timer
        .clone()
        .start(TimerMode::Repeated, TICK_INTERVAL, move || {
            let Some(position) = ctx
                .project_runtime
                .borrow()
                .as_ref()
                .map(|rt| rt.metronome_shared().position())
            else {
                return;
            };
            ctx.for_each_bridge(|bridge| {
                bridge.set_current_beat(position.beat as i32);
                bridge.set_counting_in(position.counting_in);
            });
        });
}

#[cfg(test)]
#[path = "metronome_runtime_tests.rs"]
mod runtime_tests;
