//! #791 — Tone Doctor closures for the chain windows (main + compact).
//!
//! The diagnosis is EXPENSIVE (re-renders the chain many times, NAM reloads
//! from disk) so it runs on a background thread; the result is marshalled back
//! with `invoke_from_event_loop`.
//!
//! Signal source, gated (the user's rule "a DI must be running OR the chain
//! must be active"):
//!   - a DI is selected for the chain → render that DI file, analyse the OUTPUT.
//!   - else the chain has a live runtime → capture N seconds of the player's
//!     guitar from the input tap, render it, analyse the OUTPUT.
//!   - neither → the panel shows "play a DI or enable the chain".
//! The measurement is ALWAYS the chain output (the whole tone). The user picks
//! N (the analyse duration) in the panel.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use application::di_loader::load_di_loop;
use application::dispatcher::CommandDispatcher;
use engine::spsc::SpscRing;
use engine::tone_doctor_suggestion::Suggestion;
use infra_cpal::ProjectRuntimeController;
use slint::{ComponentHandle, Weak};

use crate::helpers::set_status_error;
use crate::state::ProjectSession;
use crate::tone_doctor_wiring::ToneDoctorView;
use crate::{AppWindow, CompactChainViewWindow, ToneDoctorState};

/// Fixed block size for the offline diagnosis render.
const DIAGNOSE_BLOCK: usize = 512;

type SuggestionCache = Arc<Mutex<Option<Suggestion>>>;
type ProjectRuntime = Rc<RefCell<Option<ProjectRuntimeController>>>;

/// Push a `ToneDoctorView` onto the panel's global.
fn apply_view(st: &ToneDoctorState, view: &ToneDoctorView) {
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
    st.set_clip_value(view.clip_value);
    st.set_clip_limit(view.clip_limit);
}

/// Record up to `seconds` of the mono input tap, broadcast to stereo frames.
/// Polls the lock-free ring; the player should be playing during the window.
fn record(ring: Arc<SpscRing<f32>>, sr: f32, seconds: usize) -> Vec<[f32; 2]> {
    let target = seconds * sr as usize;
    let mut mono: Vec<f32> = Vec::with_capacity(target);
    let start = Instant::now();
    let deadline = Duration::from_secs(seconds as u64 + 2);
    while mono.len() < target && start.elapsed() < deadline {
        let mut got = false;
        while let Some(s) = ring.pop() {
            mono.push(s);
            got = true;
            if mono.len() >= target {
                break;
            }
        }
        if !got {
            std::thread::sleep(Duration::from_millis(15));
        }
    }
    mono.into_iter().map(|s| [s, s]).collect()
}

/// Run the diagnosis on a background thread. `produce` yields the input frames +
/// sample rate (DI decode or live capture — both blocking, hence off-thread).
fn spawn<F, D>(chain: project::chain::Chain, cache: SuggestionCache, seconds: usize, produce: F, on_done: D)
where
    F: FnOnce() -> Option<(Vec<[f32; 2]>, f32)> + Send + 'static,
    D: FnOnce(ToneDoctorView) + Send + 'static,
{
    std::thread::spawn(move || {
        let view = match produce() {
            Some((mut input, sr)) => {
                let cap = seconds * sr as usize;
                if input.len() > cap {
                    input.truncate(cap);
                }
                let (view, suggestion) =
                    crate::tone_doctor_wiring::diagnose_to_view(&chain, &input, sr, DIAGNOSE_BLOCK);
                if let Ok(mut c) = cache.lock() {
                    *c = suggestion;
                }
                view
            }
            None => ToneDoctorView::default(),
        };
        let _ = slint::invoke_from_event_loop(move || on_done(view));
    });
}

/// Resolve the culprit's chain by index.
fn resolve_chain(
    session: &ProjectSession,
    chain_index: i32,
) -> Option<(project::chain::Chain, domain::ids::ChainId)> {
    let proj = session.project.borrow();
    proj.chains
        .get(chain_index as usize)
        .map(|c| (c.clone(), c.id.clone()))
}

/// Kick off a run: flip the panel to running, pick the signal source (DI or
/// live capture) per the gate, and spawn. On no signal, disable the button.
fn start_run(
    st: &ToneDoctorState,
    session: &ProjectSession,
    project_runtime: &ProjectRuntime,
    chain_index: i32,
    cache: SuggestionCache,
    on_done: impl FnOnce(ToneDoctorView) + Send + 'static,
) {
    st.set_running(true);
    let seconds = (st.get_analyze_seconds().max(1)) as usize;
    let Some((chain, chain_id)) = resolve_chain(session, chain_index) else {
        st.set_running(false);
        return;
    };

    // 1) a DI selected for the chain → render that file.
    if let Some(source) = session.dispatcher.di_loop_source_for_chain(&chain_id) {
        st.set_can_diagnose(true);
        st.set_source_kind("di".into());
        spawn(chain, cache, seconds, move || {
            load_di_loop(&source)
                .ok()
                .map(|di| (di.stereo_frames(), di.src_sr() as f32))
        }, on_done);
        return;
    }

    // 2) the chain is live → capture N seconds of the input tap.
    let tap = project_runtime.borrow().as_ref().and_then(|rt| {
        let sr = rt.sample_rate();
        rt.subscribe_stream_input_tap(&chain_id, 0, seconds * sr as usize)
            .map(|ring| (ring, sr as f32))
    });
    if let Some((ring, sr)) = tap {
        st.set_can_diagnose(true);
        st.set_source_kind("live".into());
        spawn(chain, cache, seconds, move || Some((record(ring, sr, seconds), sr)), on_done);
        return;
    }

    // 3) neither → nothing to analyse.
    st.set_can_diagnose(false);
    st.set_source_kind(slint::SharedString::new());
    st.set_running(false);
}

/// Dispatch the cached suggestion (enable gate + set number) for `chain_index`.
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
    let Some((chain_clone, chain_id)) = resolve_chain(session, chain_index) else {
        return;
    };
    for cmd in crate::tone_doctor_wiring::apply_commands(&chain_clone, &chain_id, &suggestion) {
        if let Err(err) = session.dispatcher.dispatch(cmd) {
            if let Some(main_win) = main_weak.upgrade() {
                set_status_error(&main_win, toast_timer, &err.to_string());
            }
            return;
        }
    }
}

/// Wire the compact window's Tone Doctor run/apply for its single `chain_index`.
pub(crate) fn wire(
    compact_win: &CompactChainViewWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    project_runtime: ProjectRuntime,
    chain_index: i32,
    main_weak: Weak<AppWindow>,
    toast_timer: Rc<slint::Timer>,
) {
    let cache: SuggestionCache = Arc::new(Mutex::new(None));
    {
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        let cache = cache.clone();
        let weak = compact_win.as_weak();
        compact_win.on_tone_doctor_run(move |_ci| {
            let Some(win) = weak.upgrade() else {
                return;
            };
            let st = win.global::<ToneDoctorState>();
            let sb = project_session.borrow();
            let Some(session) = sb.as_ref() else {
                st.set_running(false);
                return;
            };
            let w2 = win.as_weak();
            start_run(&st, session, &project_runtime, chain_index, cache.clone(), move |view| {
                if let Some(win) = w2.upgrade() {
                    apply_view(&win.global::<ToneDoctorState>(), &view);
                }
            });
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

/// Wire the main chains page's Tone Doctor run/apply (chain chosen per click).
pub(crate) fn wire_main(
    window: &AppWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    project_runtime: ProjectRuntime,
    toast_timer: Rc<slint::Timer>,
) {
    let cache: SuggestionCache = Arc::new(Mutex::new(None));
    let main_weak = window.as_weak();
    {
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        let cache = cache.clone();
        let weak = window.as_weak();
        window.on_tone_doctor_run(move |ci| {
            let Some(win) = weak.upgrade() else {
                return;
            };
            let st = win.global::<ToneDoctorState>();
            let sb = project_session.borrow();
            let Some(session) = sb.as_ref() else {
                st.set_running(false);
                return;
            };
            let w2 = win.as_weak();
            start_run(&st, session, &project_runtime, ci, cache.clone(), move |view| {
                if let Some(win) = w2.upgrade() {
                    apply_view(&win.global::<ToneDoctorState>(), &view);
                }
            });
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
