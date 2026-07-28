//! #791 — deliver `Event::ChainToneDiagnosed` to whichever window is showing
//! the Tone Doctor panel.
//!
//! The diagnosis runs on the command bus now, so its result comes back as an
//! event on the frontend drain rather than as a value the panel's own thread
//! produced. Windows register here when their panel is wired; the drain calls
//! [`apply`] once per tick.

use std::cell::RefCell;

use application::event::Event;
use slint::{ComponentHandle, Weak};

use crate::tone_doctor_wiring::{view_from_report, ToneDoctorView};
use crate::{AppWindow, CompactChainViewWindow, ToneDoctorState};

thread_local! {
    /// The windows whose `ToneDoctorState` should follow the verdicts. Weak, so
    /// a closed compact window drops out on its own.
    static MAIN: RefCell<Option<Weak<AppWindow>>> = const { RefCell::new(None) };
    static COMPACT: RefCell<Option<Weak<CompactChainViewWindow>>> = const { RefCell::new(None) };
}

/// Push a `ToneDoctorView` onto a window's panel global.
pub(crate) fn apply_view(st: &ToneDoctorState, view: &ToneDoctorView) {
    st.set_running(view.running);
    st.set_has_result(view.has_result);
    st.set_symptom_level(view.symptom_level);
    st.set_symptom_text(view.symptom_text.clone().into());
    st.set_culprit_label(view.culprit_label.clone().into());
    st.set_has_suggestion(view.has_suggestion);
    st.set_suggestion_text(view.suggestion_text.clone().into());
    st.set_fizz_value(view.fizz_value);
    st.set_fizz_limit(view.fizz_limit);
    st.set_mud_value(view.mud_value);
    st.set_mud_limit(view.mud_limit);
    st.set_boom_value(view.boom_value);
    st.set_boom_limit(view.boom_limit);
    st.set_clip_value(view.clip_value);
    st.set_clip_limit(view.clip_limit);
}

pub(crate) fn register_main(window: Weak<AppWindow>) {
    MAIN.with(|m| *m.borrow_mut() = Some(window));
}

pub(crate) fn register_compact(window: Weak<CompactChainViewWindow>) {
    COMPACT.with(|c| *c.borrow_mut() = Some(window));
}

/// Apply every diagnosis in `events` to the registered windows. A failed run
/// (`Event::Error` naming the doctor) clears the running flag so the panel does
/// not spin forever.
pub(crate) fn apply(events: &[Event]) {
    for ev in events {
        match ev {
            Event::ChainToneDiagnosed { report, .. } => {
                let view = view_from_report(report);
                MAIN.with(|m| {
                    if let Some(win) = m.borrow().as_ref().and_then(|w| w.upgrade()) {
                        apply_view(&win.global::<ToneDoctorState>(), &view);
                    }
                });
                COMPACT.with(|c| {
                    if let Some(win) = c.borrow().as_ref().and_then(|w| w.upgrade()) {
                        apply_view(&win.global::<ToneDoctorState>(), &view);
                    }
                });
            }
            Event::Error { message } if message.starts_with("tone diagnosis failed") => {
                stop_running();
            }
            _ => {}
        }
    }
}

/// Clear the panel's spinner on both windows and fall back to the "nothing to
/// analyse" line — a run that failed (silent window, render error) must not
/// leave the previous verdict standing as if it were fresh.
pub(crate) fn stop_running() {
    let clear = |st: &ToneDoctorState| {
        st.set_running(false);
        st.set_can_diagnose(false);
    };
    MAIN.with(|m| {
        if let Some(win) = m.borrow().as_ref().and_then(|w| w.upgrade()) {
            clear(&win.global::<ToneDoctorState>());
        }
    });
    COMPACT.with(|c| {
        if let Some(win) = c.borrow().as_ref().and_then(|w| w.upgrade()) {
            clear(&win.global::<ToneDoctorState>());
        }
    });
}
