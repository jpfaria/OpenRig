//! #791 — Tone Doctor closures for the chain windows (main + compact).
//!
//! The diagnosis is EXPENSIVE (it re-renders the chain several times, rebuilding
//! every block — NAM models reload from disk per render). It MUST NOT run on the
//! Slint/GUI thread or the app freezes on the Diagnose click. So the click only
//! resolves the chain + DI source (cheap) on the UI thread, flips the panel to
//! "running", and hands the heavy work to a background thread; the result is
//! marshalled back with `invoke_from_event_loop`.
//!
//! The pure mapping (diagnose → view, suggestion → command) lives in the
//! unit-tested `tone_doctor_wiring`; this file is the threading + Slint glue.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use application::di_loader::{load_di_loop, DiLoopSource};
use application::dispatcher::CommandDispatcher;
use engine::tone_doctor_suggestion::Suggestion;
use project::chain::Chain;
use slint::{ComponentHandle, Weak};

use crate::helpers::set_status_error;
use crate::state::ProjectSession;
use crate::{AppWindow, CompactChainViewWindow, ToneDoctorState};

/// Fixed block size for the offline diagnosis render.
const DIAGNOSE_BLOCK: usize = 512;
/// Cap the DI fed to the diagnosis (seconds). A few seconds of the player's
/// take is plenty to detect the symptom, and it keeps the N+1 renders bounded.
const DIAGNOSE_MAX_SECS: f32 = 3.0;

/// Suggestion cached between a run and a later Apply. `Arc<Mutex<_>>` (not
/// `Rc<RefCell<_>>`) so the background thread can write it.
type SuggestionCache = Arc<Mutex<Option<Suggestion>>>;

/// Push a `ToneDoctorView` onto the panel's global.
fn apply_view(st: &ToneDoctorState, view: &crate::tone_doctor_wiring::ToneDoctorView) {
    st.set_running(view.running);
    st.set_has_result(view.has_result);
    st.set_symptom_level(view.symptom_level);
    st.set_symptom_text(view.symptom_text.clone().into());
    st.set_culprit_label(view.culprit_label.clone().into());
    st.set_has_suggestion(view.has_suggestion);
    st.set_suggestion_text(view.suggestion_text.clone().into());
}

/// Start a diagnosis off the GUI thread. `chain` + `source` are already
/// resolved on the UI thread; the heavy DI decode + ablation happen here, then
/// `on_done` runs back on the UI thread with the finished view.
fn spawn_diagnosis<F>(chain: Chain, source: DiLoopSource, cache: SuggestionCache, on_done: F)
where
    F: FnOnce(crate::tone_doctor_wiring::ToneDoctorView) + Send + 'static,
{
    std::thread::spawn(move || {
        let view = match load_di_loop(&source) {
            Ok(di) => {
                let mut input = di.stereo_frames();
                let sr = di.src_sr() as f32;
                let cap = (DIAGNOSE_MAX_SECS * sr) as usize;
                if input.len() > cap {
                    input.truncate(cap);
                }
                let (view, suggestion) = crate::tone_doctor_wiring::diagnose_to_view(
                    &chain,
                    &input,
                    sr,
                    DIAGNOSE_BLOCK,
                );
                if let Ok(mut c) = cache.lock() {
                    *c = suggestion;
                }
                view
            }
            // DI failed to load — clear the spinner, leave no result.
            Err(_) => crate::tone_doctor_wiring::ToneDoctorView::default(),
        };
        let _ = slint::invoke_from_event_loop(move || on_done(view));
    });
}

/// Resolve `(chain, DI source)` for `chain_index` from the session. `Ok(None)`
/// means "no DI selected"; `Err(())` means the chain/session is gone.
#[allow(clippy::result_unit_err)]
fn resolve(
    session: &ProjectSession,
    chain_index: i32,
) -> Result<Option<(Chain, DiLoopSource)>, ()> {
    let (chain, chain_id) = {
        let proj = session.project.borrow();
        let c = proj.chains.get(chain_index as usize).ok_or(())?;
        (c.clone(), c.id.clone())
    };
    Ok(session
        .dispatcher
        .di_loop_source_for_chain(&chain_id)
        .map(|src| (chain, src)))
}

/// Show the "select a DI first" amber line on the panel.
fn set_no_di(st: &ToneDoctorState) {
    st.set_running(false);
    st.set_has_result(true);
    st.set_symptom_level(1);
    st.set_symptom_text(rust_i18n::t!("tone-doctor-no-di").to_string().into());
    st.set_culprit_label(slint::SharedString::new());
    st.set_has_suggestion(false);
    st.set_suggestion_text(slint::SharedString::new());
}

/// Dispatch the cached suggestion's parameter change for `chain_index`.
fn apply_suggestion(
    session: &ProjectSession,
    chain_index: i32,
    cache: &SuggestionCache,
    main_weak: &Weak<AppWindow>,
    toast_timer: &Rc<slint::Timer>,
) {
    let Some(suggestion) = cache.lock().ok().and_then(|c| c.clone()) else {
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
    let cache: SuggestionCache = Arc::new(Mutex::new(None));
    {
        let project_session = project_session.clone();
        let cache = cache.clone();
        let weak = compact_win.as_weak();
        compact_win.on_tone_doctor_run(move |_ci| {
            let Some(win) = weak.upgrade() else {
                return;
            };
            let st = win.global::<ToneDoctorState>();
            st.set_running(true);
            let resolved = {
                let sb = project_session.borrow();
                let Some(session) = sb.as_ref() else {
                    st.set_running(false);
                    return;
                };
                resolve(session, chain_index)
            };
            match resolved {
                Ok(Some((chain, source))) => {
                    let w2 = win.as_weak();
                    spawn_diagnosis(chain, source, cache.clone(), move |view| {
                        if let Some(win) = w2.upgrade() {
                            apply_view(&win.global::<ToneDoctorState>(), &view);
                        }
                    });
                }
                Ok(None) => {
                    set_no_di(&st);
                    if let Ok(mut c) = cache.lock() {
                        *c = None;
                    }
                }
                Err(()) => st.set_running(false),
            }
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

/// Wire the main chains page's Tone Doctor run/apply. The chain is chosen per
/// click (the callback's `ci`), not fixed.
pub(crate) fn wire_main(
    window: &AppWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    toast_timer: Rc<slint::Timer>,
) {
    let cache: SuggestionCache = Arc::new(Mutex::new(None));
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
            let resolved = {
                let sb = project_session.borrow();
                let Some(session) = sb.as_ref() else {
                    st.set_running(false);
                    return;
                };
                resolve(session, ci)
            };
            match resolved {
                Ok(Some((chain, source))) => {
                    let w2 = win.as_weak();
                    spawn_diagnosis(chain, source, cache.clone(), move |view| {
                        if let Some(win) = w2.upgrade() {
                            apply_view(&win.global::<ToneDoctorState>(), &view);
                        }
                    });
                }
                Ok(None) => {
                    set_no_di(&st);
                    if let Ok(mut c) = cache.lock() {
                        *c = None;
                    }
                }
                Err(()) => st.set_running(false),
            }
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
