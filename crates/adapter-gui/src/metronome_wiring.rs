//! Responsibility: wires the metronome window.
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
//!   its `Command` and nothing else: since #127 the dispatcher OWNS the
//!   metronome — it validates the value, remembers it, persists it and applies
//!   it to the audio runtime through `RuntimeControl`. This file only mirrors
//!   the result onto the screen, which is why it no longer holds the audio
//!   backend: a footswitch or an MCP client now starts the same click the knob
//!   does.
//! * **The lamps read a phase, not a queue.** The timer samples the click's
//!   position through [`LiveSource`], so a frame that arrives late shows the
//!   beat the click is actually on instead of replaying a backlog. It also
//!   re-renders whenever the dispatcher's snapshot changed, so a tempo set from
//!   another transport lands on the knob while the panel is open.

use std::cell::RefCell;
use std::rc::Rc;

use application::live_source::LiveSource;
use application::metronome_state::MetronomeSnapshot;
use slint::{ComponentHandle, Global, Timer, TimerMode};

use crate::helpers::{show_child_window, use_inline_block_editor};
use crate::metronome_controls_wiring::{
    refresh_metronome_outputs, wire_controls, wire_output_select,
};
use crate::metronome_events::{dispatch, render_settings};
use crate::metronome_view::MetronomeOutput;
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
    /// #127: the read seam. The click's beat position and whether it is
    /// sounding come from here — never from the audio backend directly, which
    /// is the whole point of the issue.
    pub(crate) live: Rc<dyn LiveSource>,
    pub(crate) timer: Rc<Timer>,
    /// The standalone window (windowed desktop mode).
    pub(crate) window: slint::Weak<MetronomeWindow>,
    /// The main window, whose bridge drives the inline panel (fullscreen/touch).
    pub(crate) main_window: slint::Weak<AppWindow>,
    /// The project's output endpoints as published to the select, cached so
    /// each keystroke filters the list instead of re-reading the bindings.
    pub(crate) outputs: Rc<RefCell<Vec<MetronomeOutput>>>,
    /// The snapshot the knobs currently show. The lamp timer re-renders only
    /// when it differs, so mirroring a change made on another transport costs
    /// one comparison per frame instead of a full label rebuild.
    pub(crate) rendered: Rc<RefCell<Option<MetronomeSnapshot>>>,
}

impl MetronomeCtx {
    pub(crate) fn clone_ctx(&self) -> Self {
        Self {
            project_session: self.project_session.clone(),
            live: self.live.clone(),
            timer: self.timer.clone(),
            window: self.window.clone(),
            main_window: self.main_window.clone(),
            outputs: self.outputs.clone(),
            rendered: self.rendered.clone(),
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

    /// Whether the click is sounding, as the AUDIO side reports it. `false`
    /// with no runtime hosted: nothing can be playing then.
    pub(crate) fn click_is_playing(&self) -> bool {
        self.live.metronome().is_some_and(|click| click.running)
    }
}

/// Wire every metronome callback (open / close / power / controls) onto the
/// supplied windows. Call once per `AppWindow + MetronomeWindow` pair.
pub fn wire_metronome(
    window: &AppWindow,
    metronome_window: &MetronomeWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    live: &Rc<dyn LiveSource>,
    metronome_timer: &Rc<Timer>,
) {
    let ctx = MetronomeCtx {
        project_session: project_session.clone(),
        live: live.clone(),
        timer: metronome_timer.clone(),
        window: metronome_window.as_weak(),
        main_window: window.as_weak(),
        outputs: Rc::new(RefCell::new(Vec::new())),
        rendered: Rc::new(RefCell::new(None)),
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
    crate::AnalyzerBridge::get(window).on_open_metronome_window(move || {
        let Some(main_w) = main_window_weak.upgrade() else {
            return;
        };
        // Show the CURRENT state: the settings the dispatcher holds, and POWER
        // reflecting whether the click is actually playing — closing the window
        // only hides it (the click keeps going), so reopening must not look
        // stopped.
        render_settings(&ctx);
        set_power_display(&ctx, ctx.click_is_playing());
        // The lamps only mean anything while a surface is visible, so the timer
        // lives exactly as long as one is. It is also what mirrors a change made
        // from MCP or a footswitch while the panel is open.
        start_lamp_timer(&ctx);

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
    // the standalone window (hide it). Do NOT stop the click.
    if let Some(aw) = ctx.main_window.upgrade() {
        MetronomeBridge::get(&aw).set_show(false);
    }
    if let Some(mw) = ctx.window.upgrade() {
        let _ = mw.hide();
    }
    // Nothing renders the lamps once both surfaces are hidden, so stop reading
    // them. The click is unaffected — it lives in its own stream.
    ctx.timer.stop();
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
        // The dispatcher starts and stops the click itself (#127) — including
        // creating the audio runtime when no chain is enabled (#808), because
        // the metronome is an independent pipeline (invariant #4).
        dispatch(
            &ctx,
            Command::Metronome(MetronomeCommand::SetMetronomeEnabled { enabled }),
        );
        // The switch follows the DISPATCHER, which records a start only once
        // the runtime accepted it — so a click with no endpoint to play
        // through does not leave POWER lit over silence. With no project open
        // there is no dispatcher to ask, and the request itself is the best
        // answer: the switch must never look stuck.
        let lit = snapshot(&ctx).map_or(enabled, |state| state.running);
        set_power_display(&ctx, lit);
    });
}

/// Sample the click's position onto the beat lamps, and keep the knobs in step
/// with the dispatcher.
///
/// Reading the phase (not a queue of beat events) is what makes a late frame
/// harmless. Reading it through [`LiveSource`] is what keeps this module off
/// the audio backend.
pub(crate) fn start_lamp_timer(ctx: &MetronomeCtx) {
    let ctx = ctx.clone_ctx();
    ctx.timer
        .clone()
        .start(TimerMode::Repeated, TICK_INTERVAL, move || {
            let Some(click) = ctx.live.metronome() else {
                return;
            };
            ctx.for_each_bridge(|bridge| {
                bridge.set_metronome_enabled(click.running);
                bridge.set_current_beat(if click.running { click.beat as i32 } else { 0 });
                bridge.set_counting_in(click.running && click.counting_in);
            });
            // A tempo (or timbre, or output) set from MCP or a MIDI CC changes
            // the dispatcher's snapshot without any event reaching this window.
            // One comparison a frame is what makes the knobs follow it.
            //
            // The borrow ends on its own line on purpose: `render_settings`
            // borrows `rendered` mutably, so holding this one into the call
            // would panic.
            let current = snapshot(&ctx);
            let changed = ctx.rendered.borrow().as_ref() != current.as_ref();
            if changed {
                render_settings(&ctx);
            }
        });
}

/// The metronome state the dispatcher owns. `None` with no project open —
/// there is no dispatcher to ask.
pub(crate) fn snapshot(ctx: &MetronomeCtx) -> Option<MetronomeSnapshot> {
    ctx.project_session
        .borrow()
        .as_ref()
        .map(|session| session.dispatcher.metronome_snapshot())
}

/// Re-read the project's endpoints, keeping the select's cache in step.
pub(crate) fn outputs(ctx: &MetronomeCtx) -> Vec<MetronomeOutput> {
    refresh_metronome_outputs(&ctx.project_session, &ctx.outputs)
}
