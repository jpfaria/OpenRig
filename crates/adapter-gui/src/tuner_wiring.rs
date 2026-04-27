//! TunerWindow wiring — all callbacks and the polling timer for the
//! top-bar Tuner feature. Extracted out of `adapter-gui/src/lib.rs`
//! (god file, issue #276) so a feature lives in its own file.
//!
//! `wire_tuner` is the single entry point. `lib.rs` calls it once during
//! window setup and never touches tuner logic again.

use std::cell::RefCell;
use std::rc::Rc;

use infra_cpal::ProjectRuntimeController;
use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};

use crate::helpers::{show_child_window, use_inline_block_editor};
use crate::state::ProjectSession;
use crate::tuner_session::TunerSession;
use crate::{AppWindow, TunerRow, TunerWindow};

const TICK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(33);

/// Wire every Tuner callback (open / close / mute / power) onto the
/// supplied windows. Idempotent in spirit but should only be called
/// once per `AppWindow + TunerWindow` pair during application startup.
pub fn wire_tuner(
    window: &AppWindow,
    tuner_window: &TunerWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    tuner_session: &Rc<RefCell<Option<TunerSession>>>,
    tuner_timer: &Rc<Timer>,
) {
    wire_open(window, tuner_window);
    wire_close_inline(window, project_runtime, tuner_session, tuner_timer);
    wire_close_windowed(tuner_window, project_runtime, tuner_session, tuner_timer);
    wire_mute_inline(window, project_runtime);
    wire_mute_windowed(tuner_window, project_runtime);
    wire_power(
        window,
        tuner_window,
        project_session,
        project_runtime,
        tuner_session,
        tuner_timer,
    );
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

fn wire_close_inline(
    window: &AppWindow,
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    tuner_session: &Rc<RefCell<Option<TunerSession>>>,
    tuner_timer: &Rc<Timer>,
) {
    let project_runtime = project_runtime.clone();
    let tuner_session = tuner_session.clone();
    let tuner_timer = tuner_timer.clone();
    let main_window_weak = window.as_weak();
    window.on_close_tuner(move || {
        teardown_session(&tuner_timer, &tuner_session, &project_runtime);
        if let Some(mw) = main_window_weak.upgrade() {
            mw.set_show_tuner(false);
            mw.set_tuner_mute_active(false);
        }
    });
}

fn wire_close_windowed(
    tuner_window: &TunerWindow,
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    tuner_session: &Rc<RefCell<Option<TunerSession>>>,
    tuner_timer: &Rc<Timer>,
) {
    let project_runtime = project_runtime.clone();
    let tuner_session = tuner_session.clone();
    let tuner_timer = tuner_timer.clone();
    let tuner_window_weak = tuner_window.as_weak();
    tuner_window.on_close_tuner_window(move || {
        teardown_session(&tuner_timer, &tuner_session, &project_runtime);
        if let Some(tw) = tuner_window_weak.upgrade() {
            tw.set_mute_active(false);
            let _ = tw.hide();
        }
    });
}

fn wire_mute_inline(
    window: &AppWindow,
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) {
    let project_runtime = project_runtime.clone();
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
        if let Some(rt) = project_runtime.borrow().as_ref() {
            rt.set_output_muted(muted);
        }
        mw.set_tuner_mute_active(muted);
    });
}

fn wire_mute_windowed(
    tuner_window: &TunerWindow,
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) {
    let project_runtime = project_runtime.clone();
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
        if let Some(rt) = project_runtime.borrow().as_ref() {
            rt.set_output_muted(muted);
        }
        // One-way `in property <bool>` — the caller has to push the new
        // value back so the toggle sprite + LED render correctly.
        tw.set_mute_active(muted);
    });
}

fn wire_power(
    window: &AppWindow,
    tuner_window: &TunerWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    tuner_session: &Rc<RefCell<Option<TunerSession>>>,
    tuner_timer: &Rc<Timer>,
) {
    let project_session = project_session.clone();
    let project_runtime = project_runtime.clone();
    let tuner_session = tuner_session.clone();
    let tuner_timer = tuner_timer.clone();
    let main_window_weak = window.as_weak();
    let tuner_window_weak = tuner_window.as_weak();

    let on_toggle_enabled = move |enabled: bool| {
        if enabled {
            let new_session = build_session(&project_session, &project_runtime);
            let rows = new_session
                .as_ref()
                .map(TunerSession::rows_model_rc)
                .unwrap_or_else(empty_rows_model);
            // Powering on auto-engages mute so the user can tune silently
            // without an extra click. They can still toggle it off after.
            if let Some(rt) = project_runtime.borrow().as_ref() {
                rt.set_output_muted(true);
            }
            // Always reflect the new enabled state on the UI, even when
            // no session could be built (no runtime / no active chain).
            // Otherwise the sprite would stay stuck at OFF and the user
            // would have to find another way to power the tuner back on.
            if let Some(tw) = tuner_window_weak.upgrade() {
                tw.set_tuner_rows(rows.clone());
                tw.set_mute_active(true);
                tw.set_tuner_enabled(true);
            }
            if let Some(mw) = main_window_weak.upgrade() {
                mw.set_tuner_rows(rows);
                mw.set_tuner_mute_active(true);
                mw.set_tuner_enabled(true);
            }
            *tuner_session.borrow_mut() = new_session;
            start_polling_timer(
                &tuner_timer,
                &tuner_session,
                &project_session,
                &project_runtime,
                &tuner_window_weak,
                &main_window_weak,
            );
        } else {
            teardown_session(&tuner_timer, &tuner_session, &project_runtime);
            // Power off also clears the row list and mute toggle so the
            // window reflects the "stopped" state instead of stale rows
            // or a stuck red LED.
            let empty = empty_rows_model();
            if let Some(tw) = tuner_window_weak.upgrade() {
                tw.set_tuner_rows(empty.clone());
                tw.set_mute_active(false);
                tw.set_tuner_enabled(false);
            }
            if let Some(mw) = main_window_weak.upgrade() {
                mw.set_tuner_rows(empty);
                mw.set_tuner_mute_active(false);
                mw.set_tuner_enabled(false);
            }
        }
    };
    let cloned = on_toggle_enabled.clone();
    window.on_toggle_tuner_enabled(move |e| cloned(e));
    tuner_window.on_toggle_enabled(move |e| on_toggle_enabled(e));
}

// ── helpers ─────────────────────────────────────────────────────────────

fn build_session(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Option<TunerSession> {
    let pj = project_session.borrow();
    let rt = project_runtime.borrow();
    match (pj.as_ref(), rt.as_ref()) {
        (Some(session), Some(runtime)) => Some(TunerSession::build(&session.project, runtime)),
        _ => None,
    }
}

fn empty_rows_model() -> ModelRc<TunerRow> {
    ModelRc::from(Rc::new(VecModel::from(Vec::<TunerRow>::new())))
}

fn teardown_session(
    tuner_timer: &Rc<Timer>,
    tuner_session: &Rc<RefCell<Option<TunerSession>>>,
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) {
    tuner_timer.stop();
    *tuner_session.borrow_mut() = None;
    if let Some(rt) = project_runtime.borrow().as_ref() {
        rt.prune_dead_input_taps();
        rt.set_output_muted(false);
    }
}

/// Drive the per-frame loop: drain rings, run YIN detection, and rebuild
/// the session if the project's input topology changed under us.
fn start_polling_timer(
    tuner_timer: &Rc<Timer>,
    tuner_session: &Rc<RefCell<Option<TunerSession>>>,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    tuner_window_weak: &slint::Weak<TunerWindow>,
    main_window_weak: &slint::Weak<AppWindow>,
) {
    let tuner_session = tuner_session.clone();
    let project_session = project_session.clone();
    let project_runtime = project_runtime.clone();
    let tuner_window_weak = tuner_window_weak.clone();
    let main_window_weak = main_window_weak.clone();
    tuner_timer.start(TimerMode::Repeated, TICK_INTERVAL, move || {
        if let Some(session) = tuner_session.borrow_mut().as_mut() {
            session.tick();
        }
        let needs_rebuild = {
            let pj = project_session.borrow();
            let session = tuner_session.borrow();
            match (pj.as_ref(), session.as_ref()) {
                (Some(s), Some(sess)) => sess.needs_rebuild(&s.project),
                (Some(_), None) => true,
                _ => false,
            }
        };
        if needs_rebuild {
            let pj = project_session.borrow();
            let rt = project_runtime.borrow();
            if let (Some(s), Some(rt)) = (pj.as_ref(), rt.as_ref()) {
                let new_session = TunerSession::build(&s.project, rt);
                let rows = new_session.rows_model_rc();
                if let Some(tw) = tuner_window_weak.upgrade() {
                    tw.set_tuner_rows(rows.clone());
                }
                if let Some(mw) = main_window_weak.upgrade() {
                    mw.set_tuner_rows(rows);
                }
                *tuner_session.borrow_mut() = Some(new_session);
                rt.prune_dead_input_taps();
            }
        }
    });
}
