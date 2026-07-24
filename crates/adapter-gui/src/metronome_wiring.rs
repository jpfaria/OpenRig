//! MetronomeWindow wiring (#14) — the single entry point plus the open, close
//! and power paths for the top-bar metronome. Mirrors `tuner_wiring.rs`.
//!
//! The controls live in `metronome_controls_wiring.rs` and the dispatch/event
//! application in `metronome_events.rs`; two rules shape all three:
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

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use slint::{ComponentHandle, Timer, TimerMode};

use crate::helpers::show_child_window;
use crate::metronome_close::metronome_close_commands;
use crate::metronome_controls_wiring::{output_device_ids, wire_controls, wire_output_select};
use crate::metronome_events::{dispatch, render_settings};
use crate::metronome_session::{resolve_output_device, MetronomeSession};
use crate::state::ProjectSession;
use crate::{AppWindow, MetronomeWindow};

use application::command::{Command, MetronomeCommand};

/// Beat-lamp refresh. Fast enough that the lamp lands with the click at any
/// tempo the generator supports, cheap enough to be a single atomic load.
const TICK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(33);

/// Everything the metronome callbacks need to reach. Grouped because there are
/// eleven callbacks and they all want the same five things.
pub(crate) struct MetronomeCtx {
    pub(crate) project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub(crate) project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub(crate) session: Rc<RefCell<MetronomeSession>>,
    pub(crate) timer: Rc<Timer>,
    pub(crate) window: slint::Weak<MetronomeWindow>,
    /// Output devices as published to the select, cached so each keystroke
    /// filters the list instead of re-enumerating the host.
    pub(crate) devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
}

impl MetronomeCtx {
    pub(crate) fn clone_ctx(&self) -> Self {
        Self {
            project_session: self.project_session.clone(),
            project_runtime: self.project_runtime.clone(),
            session: self.session.clone(),
            timer: self.timer.clone(),
            window: self.window.clone(),
            devices: self.devices.clone(),
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
        devices: Rc::new(RefCell::new(Vec::new())),
    };

    wire_open(window, metronome_window, &ctx);
    wire_close(metronome_window, &ctx);
    wire_power(metronome_window, &ctx);
    wire_controls(metronome_window, &ctx);
    wire_output_select(metronome_window, &ctx);
}

// ── open / close ────────────────────────────────────────────────────────

fn wire_open(window: &AppWindow, metronome_window: &MetronomeWindow, ctx: &MetronomeCtx) {
    let metronome_window_weak = metronome_window.as_weak();
    let main_window_weak = window.as_weak();
    let ctx = ctx.clone_ctx();
    window.on_open_metronome_window(move || {
        let (Some(mw), Some(main_w)) =
            (metronome_window_weak.upgrade(), main_window_weak.upgrade())
        else {
            return;
        };
        // Open in the resting state, like the tuner: the persisted settings are
        // on screen but the click is off until the user presses POWER.
        render_settings(&mw, &ctx);
        mw.set_metronome_enabled(false);
        mw.set_current_beat(0);
        mw.set_counting_in(false);
        show_child_window(main_w.window(), mw.window());
    });
}

fn wire_close(metronome_window: &MetronomeWindow, ctx: &MetronomeCtx) {
    // The in-panel close button only exists when `MetronomePanel` renders with
    // `show-close-button: true`; in windowed mode the only way out is the OS
    // chrome, which Slint routes through `on_close_requested`. Wire BOTH so
    // neither path leaves the click playing to a hidden window (#544's lesson).
    {
        let ctx = ctx.clone_ctx();
        metronome_window.on_close_metronome_window(move || {
            close_metronome(&ctx);
            if let Some(mw) = ctx.window.upgrade() {
                let _ = mw.hide();
            }
        });
    }
    {
        let ctx = ctx.clone_ctx();
        metronome_window.window().on_close_requested(move || {
            close_metronome(&ctx);
            slint::CloseRequestResponse::HideWindow
        });
    }
}

fn close_metronome(ctx: &MetronomeCtx) {
    for cmd in metronome_close_commands() {
        dispatch(ctx, cmd);
    }
    // Defense in depth: with no project session open the dispatch above is a
    // no-op, so stop the stream here too rather than trust the event path.
    stop_click(ctx);
    if let Some(mw) = ctx.window.upgrade() {
        mw.set_metronome_enabled(false);
        mw.set_current_beat(0);
        mw.set_counting_in(false);
    }
}

// ── power ───────────────────────────────────────────────────────────────

fn wire_power(metronome_window: &MetronomeWindow, ctx: &MetronomeCtx) {
    let ctx = ctx.clone_ctx();
    metronome_window.on_toggle_enabled(move |enabled| {
        dispatch(
            &ctx,
            Command::Metronome(MetronomeCommand::SetMetronomeEnabled { enabled }),
        );
        // With no project open there is no dispatcher and therefore no event —
        // reflect the request on the switch anyway so it never looks stuck.
        if let Some(mw) = ctx.window.upgrade() {
            mw.set_metronome_enabled(enabled);
        }
    });
}

/// Open the metronome's own output stream and start the lamp timer.
pub(crate) fn start_click(ctx: &MetronomeCtx) {
    let settings = ctx.session.borrow().settings();
    let saved = ctx.session.borrow().output_device().map(str::to_string);
    let device = resolve_output_device(saved.as_deref(), &output_device_ids(&ctx.devices));
    if let Some(rt) = ctx.project_runtime.borrow().as_ref() {
        rt.set_metronome_settings(settings);
        let shared = rt.metronome_shared();
        shared.set_enabled(true);
        // Start from beat one of the bar instead of wherever a previous run
        // left the phase.
        shared.request_restart();
        match device {
            Some(device_id) => {
                if let Err(e) = rt.start_metronome(&device_id) {
                    log::warn!("[metronome] start on '{device_id}' failed: {e}");
                }
            }
            None => log::warn!("[metronome] no output device available"),
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
    let project_runtime = ctx.project_runtime.clone();
    let window = ctx.window.clone();
    ctx.timer
        .start(TimerMode::Repeated, TICK_INTERVAL, move || {
            let Some(mw) = window.upgrade() else {
                return;
            };
            let Some(position) = project_runtime
                .borrow()
                .as_ref()
                .map(|rt| rt.metronome_shared().position())
            else {
                return;
            };
            mw.set_current_beat(position.beat as i32);
            mw.set_counting_in(position.counting_in);
        });
}
