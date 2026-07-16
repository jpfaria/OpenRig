//! #791 — Tone Doctor closures for the chain windows (main + compact).
//!
//! Impure glue kept out of the over-cap callback files and out of
//! `tone_doctor_wiring` (which stays pure + unit-tested). The closures delegate
//! their real work to the tested pure functions in `tone_doctor_wiring`; this
//! file only bridges to the Slint `ToneDoctorState` global, the DI loader, and
//! the dispatcher. Both the main chains page (`AppWindow`) and the detached
//! compact window (`CompactChainViewWindow`) reuse the same two helpers.

use std::cell::RefCell;
use std::rc::Rc;

use application::dispatcher::CommandDispatcher;
use engine::tone_doctor_suggestion::Suggestion;
use slint::{ComponentHandle, Weak};

use crate::helpers::set_status_error;
use crate::state::ProjectSession;
use crate::{AppWindow, CompactChainViewWindow, ToneDoctorState};

/// Fixed block size for the offline diagnosis render.
const DIAGNOSE_BLOCK: usize = 512;

/// Run the offline diagnosis for the chain at `chain_index` and write the
/// result onto `st`, caching the suggestion for a later Apply.
fn run_diagnosis(
    st: &ToneDoctorState,
    session: &ProjectSession,
    chain_index: i32,
    cache: &RefCell<Option<Suggestion>>,
) {
    let Some((chain_clone, chain_id)) = ({
        let proj = session.project.borrow();
        proj.chains
            .get(chain_index as usize)
            .map(|c| (c.clone(), c.id.clone()))
    }) else {
        st.set_running(false);
        return;
    };
    let Some(source) = session.dispatcher.di_loop_source_for_chain(&chain_id) else {
        // No DI selected → amber "select a DI" line instead of a result.
        st.set_running(false);
        st.set_has_result(true);
        st.set_symptom_level(1);
        st.set_symptom_text(rust_i18n::t!("tone-doctor-no-di").to_string().into());
        st.set_culprit_label(slint::SharedString::new());
        st.set_has_suggestion(false);
        st.set_suggestion_text(slint::SharedString::new());
        *cache.borrow_mut() = None;
        return;
    };
    let di = match application::di_loader::load_di_loop(&source) {
        Ok(d) => d,
        Err(_) => {
            st.set_running(false);
            return;
        }
    };
    let input = di.stereo_frames();
    let sr = di.src_sr() as f32;
    let (view, suggestion) =
        crate::tone_doctor_wiring::diagnose_to_view(&chain_clone, &input, sr, DIAGNOSE_BLOCK);
    *cache.borrow_mut() = suggestion;
    st.set_running(view.running);
    st.set_has_result(view.has_result);
    st.set_symptom_level(view.symptom_level);
    st.set_symptom_text(view.symptom_text.into());
    st.set_culprit_label(view.culprit_label.into());
    st.set_has_suggestion(view.has_suggestion);
    st.set_suggestion_text(view.suggestion_text.into());
}

/// Dispatch the cached suggestion's parameter change for `chain_index`.
fn apply_suggestion(
    session: &ProjectSession,
    chain_index: i32,
    cache: &RefCell<Option<Suggestion>>,
    main_weak: &Weak<AppWindow>,
    toast_timer: &Rc<slint::Timer>,
) {
    let Some(suggestion) = cache.borrow().clone() else {
        return;
    };
    let Some((chain_clone, chain_id)) = ({
        let proj = session.project.borrow();
        proj.chains
            .get(chain_index as usize)
            .map(|c| (c.clone(), c.id.clone()))
    }) else {
        return;
    };
    if let Some(cmd) = crate::tone_doctor_wiring::apply_command(&chain_clone, &chain_id, &suggestion)
    {
        if let Err(err) = session.dispatcher.dispatch(cmd) {
            if let Some(main_win) = main_weak.upgrade() {
                set_status_error(&main_win, toast_timer, &err.to_string());
            }
        }
    }
}

/// Wire the compact window's Tone Doctor run/apply for its single `chain_index`.
pub(crate) fn wire(
    compact_win: &CompactChainViewWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    chain_index: i32,
    main_weak: Weak<AppWindow>,
    toast_timer: Rc<slint::Timer>,
) {
    let cache: Rc<RefCell<Option<Suggestion>>> = Rc::new(RefCell::new(None));
    {
        let project_session = project_session.clone();
        let cache = cache.clone();
        let weak_compact = compact_win.as_weak();
        compact_win.on_tone_doctor_run(move |_ci| {
            let Some(win) = weak_compact.upgrade() else {
                return;
            };
            let st = win.global::<ToneDoctorState>();
            st.set_running(true);
            let sb = project_session.borrow();
            let Some(session) = sb.as_ref() else {
                st.set_running(false);
                return;
            };
            run_diagnosis(&st, session, chain_index, &cache);
        });
    }
    {
        let project_session = project_session;
        compact_win.on_tone_doctor_apply(move |_ci| {
            let sb = project_session.borrow();
            let Some(session) = sb.as_ref() else {
                return;
            };
            apply_suggestion(session, chain_index, &cache, &main_weak, &toast_timer);
        });
    }
}

/// Wire the main chains page's Tone Doctor run/apply. Here the chain is chosen
/// per click (the callback's `ci`), not fixed.
pub(crate) fn wire_main(
    window: &AppWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    toast_timer: Rc<slint::Timer>,
) {
    let cache: Rc<RefCell<Option<Suggestion>>> = Rc::new(RefCell::new(None));
    let main_weak = window.as_weak();
    {
        let project_session = project_session.clone();
        let cache = cache.clone();
        let weak = window.as_weak();
        window.on_tone_doctor_run(move |ci| {
            let Some(win) = weak.upgrade() else {
                return;
            };
            let st = win.global::<ToneDoctorState>();
            st.set_running(true);
            let sb = project_session.borrow();
            let Some(session) = sb.as_ref() else {
                st.set_running(false);
                return;
            };
            run_diagnosis(&st, session, ci, &cache);
        });
    }
    {
        let project_session = project_session;
        window.on_tone_doctor_apply(move |ci| {
            let sb = project_session.borrow();
            let Some(session) = sb.as_ref() else {
                return;
            };
            apply_suggestion(session, ci, &cache, &main_weak, &toast_timer);
        });
    }
}
