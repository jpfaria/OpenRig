//! TunerWindow wiring — all callbacks for the top-bar Tuner feature.
//! Extracted out of `adapter-gui/src/lib.rs` (god file, issue #276) so a
//! feature lives in its own file.
//!
//! `wire_tuner` is the single entry point. `lib.rs` calls it once during
//! window setup and never touches tuner logic again.
//!
//! #127: this module RENDERS. Powering the analyzer on and off is
//! `SelectionCommand::SetTunerEnabled` on the shared dispatcher, and the
//! session that subscribes to the audio, detects the pitch and produces the
//! rows lives behind `RuntimeControl` (`runtime_analyzers`). Before that the
//! POWER callback built it itself, so the same command from `adapter-midi`'s
//! `toggle_tuner` footswitch — or from an MCP client — flipped the mirror and
//! started nothing. The row model arrives here through the sink installed
//! below, whoever powered the tuner on.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, SelectionCommand};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::helpers::{show_child_window, use_inline_block_editor};
use crate::runtime_analyzers::AnalyzerSessions;
use crate::state::ProjectSession;
use crate::tuner_close::tuner_close_commands;
use crate::{AppWindow, TunerRow, TunerWindow};

/// Wire every Tuner callback (open / close / mute / power) onto the
/// supplied windows. Idempotent in spirit but should only be called
/// once per `AppWindow + TunerWindow` pair during application startup.
pub fn wire_tuner(
    window: &AppWindow,
    tuner_window: &TunerWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    analyzers: &AnalyzerSessions,
) {
    install_row_sink(window, tuner_window, analyzers);
    wire_open(window, tuner_window);
    wire_close_inline(window, project_session);
    wire_close_windowed(tuner_window, project_session);
    wire_mute_inline(window, project_session);
    wire_mute_windowed(tuner_window, project_session);
    wire_power(window, tuner_window, project_session);
}

/// Where the analyzer's rows are rendered. Called on every power-on AND on
/// every rebuild (the user enabled a chain, re-bound an input): the session
/// owns the model and a rebuild makes a new one, so the window has to be
/// handed the current one rather than keep the first.
fn install_row_sink(window: &AppWindow, tuner_window: &TunerWindow, analyzers: &AnalyzerSessions) {
    let main_window_weak = window.as_weak();
    let tuner_window_weak = tuner_window.as_weak();
    analyzers.on_tuner_rows(move |rows| {
        let rows = rows.unwrap_or_else(empty_rows_model);
        if let Some(tw) = tuner_window_weak.upgrade() {
            tw.set_tuner_rows(rows.clone());
        }
        if let Some(mw) = main_window_weak.upgrade() {
            mw.set_tuner_rows(rows);
        }
    });
}

fn wire_open(window: &AppWindow, tuner_window: &TunerWindow) {
    let tuner_window_weak = tuner_window.as_weak();
    let main_window_weak = window.as_weak();
    window.on_open_tuner_window(move || {
        let Some(tw) = tuner_window_weak.upgrade() else {
            return;
        };
        let Some(main_w) = main_window_weak.upgrade() else {
            return;
        };
        let inline = use_inline_block_editor(&main_w);

        // Open the tuner in the powered-off resting state: no session,
        // no polling timer, no rows, mute disengaged. The user has to
        // press POWER to start detection — that's where the session is
        // built and the timer armed (see `wire_power`).
        let empty = empty_rows_model();
        if inline {
            main_w.set_tuner_rows(empty);
            main_w.set_tuner_mute_active(false);
            main_w.set_tuner_enabled(false);
            main_w.set_show_tuner(true);
        } else {
            tw.set_tuner_rows(empty);
            tw.set_mute_active(false);
            tw.set_tuner_enabled(false);
            // Same window-opening pattern as the Block Editor: position
            // the child window relative to the main window.
            show_child_window(main_w.window(), tw.window());
        }
    });
}

fn wire_close_inline(window: &AppWindow, project_session: &Rc<RefCell<Option<ProjectSession>>>) {
    let project_session = project_session.clone();
    let main_window_weak = window.as_weak();
    window.on_close_tuner(move || {
        dispatch_close_commands(&project_session);
        if let Some(mw) = main_window_weak.upgrade() {
            mw.set_show_tuner(false);
            mw.set_tuner_mute_active(false);
            // #544: keep the power footswitch sprite in sync with the
            // backend going off. Without this, the next render of the
            // inline panel could keep the lit-on look until wire_open's
            // reset runs.
            mw.set_tuner_enabled(false);
        }
    });
}

fn wire_close_windowed(
    tuner_window: &TunerWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
) {
    // The explicit `close-tuner-window` callback is fired by the inline
    // close button — present only when TunerPanel renders with
    // `show-close-button: true`. In the standalone Window mode (which is
    // what wire_close_windowed covers) the button is hidden, so the
    // only way to close is the OS chrome (X / ⌘W). Slint routes that
    // through `Window::on_close_requested`. Wire BOTH so neither path
    // leaves the analyzer + auto-engaged mute alive (#544).
    {
        let project_session = project_session.clone();
        let tuner_window_weak = tuner_window.as_weak();
        tuner_window.on_close_tuner_window(move || {
            close_tuner_windowed_impl(&project_session, &tuner_window_weak);
            if let Some(tw) = tuner_window_weak.upgrade() {
                let _ = tw.hide();
            }
        });
    }
    {
        let project_session = project_session.clone();
        let tuner_window_weak = tuner_window.as_weak();
        tuner_window.window().on_close_requested(move || {
            close_tuner_windowed_impl(&project_session, &tuner_window_weak);
            slint::CloseRequestResponse::HideWindow
        });
    }
}

fn close_tuner_windowed_impl(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    tuner_window_weak: &slint::Weak<TunerWindow>,
) {
    dispatch_close_commands(project_session);
    if let Some(tw) = tuner_window_weak.upgrade() {
        tw.set_mute_active(false);
        tw.set_tuner_enabled(false);
    }
}

fn dispatch_close_commands(project_session: &Rc<RefCell<Option<ProjectSession>>>) {
    // #544 + architectural law "every state change is a Command": close
    // routes through the shared dispatcher so MCP / MIDI / future gRPC
    // see the tuner go off and the mute release, instead of just the
    // adapter mutating runtime state silently. #127: the same dispatch also
    // STOPS the analyzer and releases its taps.
    for cmd in tuner_close_commands() {
        dispatch(project_session, cmd);
    }
}

fn wire_mute_inline(window: &AppWindow, project_session: &Rc<RefCell<Option<ProjectSession>>>) {
    let project_session = project_session.clone();
    let main_window_weak = window.as_weak();
    window.on_toggle_tuner_mute(move |muted| {
        // Defense in depth: mute is only meaningful while the tuner is
        // powered on. If the click somehow reaches the handler with the
        // tuner off, ignore it instead of silencing the output.
        let Some(mw) = main_window_weak.upgrade() else {
            return;
        };
        if !mw.get_tuner_enabled() {
            return;
        }
        // #436 G / #127: mute is business → SelectionCommand::SetOutputMuted on
        // the shared dispatcher, which now applies the audio effect too (via
        // its RuntimeControl). The UI only renders the sprite/LED.
        dispatch_mute(&project_session, muted);
        mw.set_tuner_mute_active(muted);
    });
}

fn wire_mute_windowed(
    tuner_window: &TunerWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
) {
    let project_session = project_session.clone();
    let tuner_window_weak = tuner_window.as_weak();
    tuner_window.on_toggle_mute(move |muted| {
        // Defense in depth: mute is only meaningful while the tuner is
        // powered on. If the click somehow reaches the handler with the
        // tuner off, ignore it instead of silencing the output.
        let Some(tw) = tuner_window_weak.upgrade() else {
            return;
        };
        if !tw.get_tuner_enabled() {
            return;
        }
        // #436 G / #127: mute via SelectionCommand::SetOutputMuted (see
        // wire_mute_inline) — the dispatcher applies the audio effect.
        dispatch_mute(&project_session, muted);
        // One-way `in property <bool>` — the caller has to push the new
        // value back so the toggle sprite + LED render correctly.
        tw.set_mute_active(muted);
    });
}

fn wire_power(
    window: &AppWindow,
    tuner_window: &TunerWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
) {
    let project_session = project_session.clone();
    let main_window_weak = window.as_weak();
    let tuner_window_weak = tuner_window.as_weak();

    let on_toggle_enabled = move |enabled: bool| {
        // #436 H / #127: POWER is business → a Command on the shared
        // dispatcher, which BUILDS or TEARS DOWN the analyzer session through
        // `RuntimeControl::set_tuner_running`. The rows land on the windows
        // through the sink `install_row_sink` registered, so a footswitch and
        // an MCP client light the same tuner this button does.
        dispatch(
            &project_session,
            Command::Selection(SelectionCommand::SetTunerEnabled { enabled }),
        );
        // Powering on auto-engages mute so the user can tune silently without
        // an extra click; powering off releases it. Through the bus, so the
        // recorded mute state matches the audio for every transport.
        dispatch_mute(&project_session, enabled);
        // Always reflect the new enabled state on the UI, even when no session
        // could be built (no runtime / no active chain). Otherwise the sprite
        // would stay stuck at OFF and the user would have to find another way
        // to power the tuner back on.
        if let Some(tw) = tuner_window_weak.upgrade() {
            tw.set_mute_active(enabled);
            tw.set_tuner_enabled(enabled);
        }
        if let Some(mw) = main_window_weak.upgrade() {
            mw.set_tuner_mute_active(enabled);
            mw.set_tuner_enabled(enabled);
        }
    };
    let cloned = on_toggle_enabled.clone();
    window.on_toggle_tuner_enabled(cloned);
    tuner_window.on_toggle_enabled(on_toggle_enabled);
}

// ── helpers ─────────────────────────────────────────────────────────────

fn empty_rows_model() -> ModelRc<TunerRow> {
    ModelRc::from(Rc::new(VecModel::from(Vec::<TunerRow>::new())))
}

/// Ask the shared dispatcher to mute (or un-mute) the rig's output. Since #127
/// this is the ONLY way the tuner touches the audio: the handler owns both the
/// recorded state and the runtime effect.
fn dispatch_mute(project_session: &Rc<RefCell<Option<ProjectSession>>>, muted: bool) {
    dispatch(
        project_session,
        Command::Selection(SelectionCommand::SetOutputMuted { muted }),
    );
}

/// Clone the dispatcher handle out FIRST: dispatching applies the runtime
/// effect, and that must not run while the session cell is still borrowed.
fn dispatch(project_session: &Rc<RefCell<Option<ProjectSession>>>, cmd: Command) {
    let dispatcher = project_session
        .borrow()
        .as_ref()
        .map(|s| s.dispatcher.clone());
    let Some(dispatcher) = dispatcher else {
        return;
    };
    if let Err(e) = dispatcher.dispatch(cmd) {
        log::warn!("[tuner] command failed: {e}");
    }
}
