//! SpectrumWindow wiring — all callbacks and the polling timer for the
//! top-bar Spectrum feature. Mirrors `tuner_wiring.rs` (open / close /
//! power) minus the mute path — the spectrum view is a passive display.
//!
//! `wire_spectrum` is the single entry point. `lib.rs` calls it once
//! during window setup and never touches spectrum logic again.

use std::cell::RefCell;
use std::rc::Rc;

use infra_cpal::ProjectRuntimeController;
use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};

use crate::helpers::{show_child_window, use_inline_block_editor};
use crate::spectrum_session::SpectrumSession;
use crate::state::ProjectSession;
use crate::{AppWindow, SpectrumRow, SpectrumWindow};

const TICK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(33);

/// Wire every Spectrum callback (open / close / power) onto the supplied
/// windows. Idempotent in spirit but should only be called once per
/// `AppWindow + SpectrumWindow` pair during application startup.
pub fn wire_spectrum(
    window: &AppWindow,
    spectrum_window: &SpectrumWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    spectrum_session: &Rc<RefCell<Option<SpectrumSession>>>,
    spectrum_timer: &Rc<Timer>,
) {
    wire_open(window, spectrum_window);
    wire_close_inline(window, project_runtime, spectrum_session, spectrum_timer);
    wire_close_windowed(
        spectrum_window,
        project_runtime,
        spectrum_session,
        spectrum_timer,
    );
    wire_power(
        window,
        spectrum_window,
        project_session,
        project_runtime,
        spectrum_session,
        spectrum_timer,
    );
}

fn wire_open(window: &AppWindow, spectrum_window: &SpectrumWindow) {
    let spectrum_window_weak = spectrum_window.as_weak();
    let main_window_weak = window.as_weak();
    window.on_open_spectrum_window(move || {
        let Some(sw) = spectrum_window_weak.upgrade() else {
            return;
        };
        let Some(main_w) = main_window_weak.upgrade() else {
            return;
        };
        let inline = use_inline_block_editor(&main_w);

        // Open the spectrum in the powered-off resting state: no session,
        // no polling timer, no rows. The user has to press POWER to start
        // the analyzer (see `wire_power`).
        let empty = empty_rows_model();
        if inline {
            main_w.set_spectrum_rows(empty);
            main_w.set_spectrum_enabled(false);
            main_w.set_show_spectrum(true);
        } else {
            sw.set_spectrum_rows(empty);
            sw.set_spectrum_enabled(false);
            // Same window-opening pattern as the Block Editor: position
            // the child window relative to the main window so the user
            // sees it appear next to where they clicked instead of at
            // an OS-default location.
            show_child_window(main_w.window(), sw.window());
        }
    });
}

fn wire_close_inline(
    window: &AppWindow,
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    spectrum_session: &Rc<RefCell<Option<SpectrumSession>>>,
    spectrum_timer: &Rc<Timer>,
) {
    let project_runtime = project_runtime.clone();
    let spectrum_session = spectrum_session.clone();
    let spectrum_timer = spectrum_timer.clone();
    let main_window_weak = window.as_weak();
    window.on_close_spectrum(move || {
        teardown_session(&spectrum_timer, &spectrum_session, &project_runtime);
        if let Some(mw) = main_window_weak.upgrade() {
            mw.set_show_spectrum(false);
        }
    });
}

fn wire_close_windowed(
    spectrum_window: &SpectrumWindow,
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    spectrum_session: &Rc<RefCell<Option<SpectrumSession>>>,
    spectrum_timer: &Rc<Timer>,
) {
    let project_runtime = project_runtime.clone();
    let spectrum_session = spectrum_session.clone();
    let spectrum_timer = spectrum_timer.clone();
    let spectrum_window_weak = spectrum_window.as_weak();
    spectrum_window.on_close_spectrum_window(move || {
        teardown_session(&spectrum_timer, &spectrum_session, &project_runtime);
        if let Some(sw) = spectrum_window_weak.upgrade() {
            let _ = sw.hide();
        }
    });
}

fn wire_power(
    window: &AppWindow,
    spectrum_window: &SpectrumWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    spectrum_session: &Rc<RefCell<Option<SpectrumSession>>>,
    spectrum_timer: &Rc<Timer>,
) {
    let project_session = project_session.clone();
    let project_runtime = project_runtime.clone();
    let spectrum_session = spectrum_session.clone();
    let spectrum_timer = spectrum_timer.clone();
    let main_window_weak = window.as_weak();
    let spectrum_window_weak = spectrum_window.as_weak();

    let on_toggle_enabled = move |enabled: bool| {
        if enabled {
            let new_session = build_session(&project_session, &project_runtime);
            let rows = new_session
                .as_ref()
                .map(SpectrumSession::rows_model_rc)
                .unwrap_or_else(empty_rows_model);
            // Always reflect the new enabled state on the UI even if no
            // session could be built, so the toggle never traps OFF.
            if let Some(sw) = spectrum_window_weak.upgrade() {
                sw.set_spectrum_rows(rows.clone());
                sw.set_spectrum_enabled(true);
            }
            if let Some(mw) = main_window_weak.upgrade() {
                mw.set_spectrum_rows(rows);
                mw.set_spectrum_enabled(true);
            }
            *spectrum_session.borrow_mut() = new_session;
            start_polling_timer(
                &spectrum_timer,
                &spectrum_session,
                &project_session,
                &project_runtime,
                &spectrum_window_weak,
                &main_window_weak,
            );
        } else {
            teardown_session(&spectrum_timer, &spectrum_session, &project_runtime);
            // Power off clears the row list so the window reflects the
            // stopped state instead of stale rows.
            let empty = empty_rows_model();
            if let Some(sw) = spectrum_window_weak.upgrade() {
                sw.set_spectrum_rows(empty.clone());
                sw.set_spectrum_enabled(false);
            }
            if let Some(mw) = main_window_weak.upgrade() {
                mw.set_spectrum_rows(empty);
                mw.set_spectrum_enabled(false);
            }
        }
    };
    let cloned = on_toggle_enabled.clone();
    window.on_toggle_spectrum_enabled(move |e| cloned(e));
    spectrum_window.on_toggle_enabled(move |e| on_toggle_enabled(e));
}

// ── helpers ─────────────────────────────────────────────────────────────

fn build_session(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Option<SpectrumSession> {
    let pj = project_session.borrow();
    let rt = project_runtime.borrow();
    match (pj.as_ref(), rt.as_ref()) {
        (Some(session), Some(runtime)) => {
            Some(SpectrumSession::build(&session.project, runtime))
        }
        _ => None,
    }
}

fn empty_rows_model() -> ModelRc<SpectrumRow> {
    ModelRc::from(Rc::new(VecModel::from(Vec::<SpectrumRow>::new())))
}

fn teardown_session(
    spectrum_timer: &Rc<Timer>,
    spectrum_session: &Rc<RefCell<Option<SpectrumSession>>>,
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) {
    spectrum_timer.stop();
    *spectrum_session.borrow_mut() = None;
    if let Some(rt) = project_runtime.borrow().as_ref() {
        rt.prune_dead_stream_taps();
    }
}

/// Drive the per-frame loop: drain rings, run the FFT, rebuild the session
/// when the project's output topology changed under us.
fn start_polling_timer(
    spectrum_timer: &Rc<Timer>,
    spectrum_session: &Rc<RefCell<Option<SpectrumSession>>>,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    spectrum_window_weak: &slint::Weak<SpectrumWindow>,
    main_window_weak: &slint::Weak<AppWindow>,
) {
    let spectrum_session = spectrum_session.clone();
    let project_session = project_session.clone();
    let project_runtime = project_runtime.clone();
    let spectrum_window_weak = spectrum_window_weak.clone();
    let main_window_weak = main_window_weak.clone();
    spectrum_timer.start(TimerMode::Repeated, TICK_INTERVAL, move || {
        if let Some(session) = spectrum_session.borrow_mut().as_mut() {
            session.tick();
        }
        let needs_rebuild = {
            let pj = project_session.borrow();
            let session = spectrum_session.borrow();
            match (pj.as_ref(), session.as_ref()) {
                (Some(s), Some(sess)) => sess.needs_rebuild(&s.project),
                (Some(_), None) => true,
                _ => false,
            }
        };
        if needs_rebuild {
            let pj = project_session.borrow();
            let rt = project_runtime.borrow();
            match (pj.as_ref(), rt.as_ref()) {
                (Some(s), Some(rt)) => {
                    let new_session = SpectrumSession::build(&s.project, rt);
                    let rows = new_session.rows_model_rc();
                    if let Some(sw) = spectrum_window_weak.upgrade() {
                        sw.set_spectrum_rows(rows.clone());
                    }
                    if let Some(mw) = main_window_weak.upgrade() {
                        mw.set_spectrum_rows(rows);
                    }
                    *spectrum_session.borrow_mut() = Some(new_session);
                    rt.prune_dead_stream_taps();
                }
                _ => {
                    // No runtime (last chain disabled, runtime torn down).
                    // Drop any stale session and clear the visible bars so
                    // the window does not freeze on the last live frame.
                    if let Some(session) = spectrum_session.borrow_mut().as_mut() {
                        session.freeze_to_zero();
                    }
                    *spectrum_session.borrow_mut() = None;
                    let empty: Rc<VecModel<SpectrumRow>> =
                        Rc::new(VecModel::from(Vec::<SpectrumRow>::new()));
                    let empty_rc = ModelRc::from(empty);
                    if let Some(sw) = spectrum_window_weak.upgrade() {
                        sw.set_spectrum_rows(empty_rc.clone());
                    }
                    if let Some(mw) = main_window_weak.upgrade() {
                        mw.set_spectrum_rows(empty_rc);
                    }
                }
            }
        }
    });
}
