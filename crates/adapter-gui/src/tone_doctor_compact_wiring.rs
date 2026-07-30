//! #791 — Tone Doctor closures for the chain windows (main + compact).
//!
//! These are dispatchers, nothing more: Diagnose and Apply are `Command`s the
//! `LocalDispatcher` owns, so MCP and gRPC reach the same doctor (the panel
//! used to run the ablation itself, which is exactly the gap that reopened
//! this issue). The verdict comes back as `Event::ChainToneDiagnosed` on the
//! frontend drain and lands on the panel via `tone_doctor_events`.
//!
//! Signal source, gated (the user's rule "a DI must be running OR the chain
//! must be active"): the dispatcher prefers the chain's loaded DI loop and
//! falls back to the live input this module registers. Neither → the command
//! errors and the panel says there is nothing to analyse. The measurement is
//! ALWAYS the chain output (the whole tone); the user picks the window length
//! (N seconds) in the panel.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use application::command::{Command, ToneDoctorCommand};
use engine::spsc::SpscRing;
use infra_cpal::ProjectRuntimeController;
use slint::{ComponentHandle, Weak};

use crate::helpers::set_status_error;
use crate::state::ProjectSession;
use crate::{AppWindow, CompactChainViewWindow, ToneDoctorState};

type ProjectRuntime = Rc<RefCell<Option<ProjectRuntimeController>>>;

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

/// Give the dispatcher a way to capture this machine's live input, so a chain
/// with no DI is diagnosable from any transport, not just from the GUI.
/// Subscribing to the tap happens on the calling thread; only the fill blocks.
fn attach_live_input(session: &ProjectSession, project_runtime: &ProjectRuntime) {
    let project_runtime = project_runtime.clone();
    session
        .dispatcher
        .attach_tone_doctor_input(Box::new(move |chain_id, seconds| {
            let rt_borrow = project_runtime.borrow();
            let rt = rt_borrow.as_ref()?;
            let sr = rt.sample_rate();
            let ring = rt.subscribe_stream_input_tap(chain_id, 0, seconds * sr as usize)?;
            Some(Box::new(move || {
                Some((record(ring, sr as f32, seconds), sr as f32))
            }))
        }));
}

/// Genres from the calibrated table matching `query` (case-insensitive
/// substring; empty/blank = all), in table order. Public for testing.
pub fn filter_genres<'a>(genres: &'a [&'a str], query: &str) -> Vec<&'a str> {
    let q = query.trim().to_lowercase();
    genres
        .iter()
        .copied()
        .filter(|g| q.is_empty() || g.to_lowercase().contains(&q))
        .collect()
}

/// The selector rows for the panel: a leading "" row (label `—`, the "no genre,
/// global defaults" choice) plus the calibrated genres matching `query`.
fn genre_options(query: &str) -> slint::ModelRc<crate::SelectOption> {
    let table = engine::tone_profile_table::ProfileTable::embedded();
    let genres = table.genres();
    let mut rows = vec![crate::SelectOption {
        key: slint::SharedString::new(),
        label: slint::SharedString::from("—"),
    }];
    rows.extend(
        filter_genres(&genres, query)
            .into_iter()
            .map(|g| crate::SelectOption {
                key: g.into(),
                label: g.into(),
            }),
    );
    slint::ModelRc::new(slint::VecModel::from(rows))
}

/// The chain id at `chain_index`, if it still exists.
fn chain_id_at(session: &ProjectSession, chain_index: i32) -> Option<domain::ids::ChainId> {
    session
        .project
        .borrow()
        .chains
        .get(chain_index as usize)
        .map(|c| c.id.clone())
}

/// Ask the dispatcher for a diagnosis. The panel flips to "running" and waits
/// for `Event::ChainToneDiagnosed`; a command error (no DI, no live chain)
/// clears it right away.
fn start_run(st: &ToneDoctorState, session: &ProjectSession, chain_index: i32) {
    let Some(chain) = chain_id_at(session, chain_index) else {
        st.set_running(false);
        return;
    };
    let seconds = st.get_analyze_seconds().max(1) as u32;
    // The player's selected genre (empty ⇒ none) picks the calibrated limits;
    // unknown/none falls back to the global defaults inside the table.
    let genre = st.get_tone_genre();
    let genre = (!genre.is_empty()).then(|| genre.to_string());

    // Which source the dispatcher will use, for the panel's label.
    let has_di = session
        .dispatcher
        .di_loop_source_for_chain(&chain)
        .is_some();
    st.set_source_kind(if has_di { "di".into() } else { "live".into() });
    st.set_running(true);

    match session
        .dispatcher
        .dispatch(Command::ToneDoctor(ToneDoctorCommand::DiagnoseChainTone {
            chain,
            genre,
            seconds: Some(seconds),
        })) {
        Ok(_) => st.set_can_diagnose(true),
        Err(_) => {
            // No DI and no live runtime: nothing to analyse.
            st.set_can_diagnose(false);
            st.set_source_kind(slint::SharedString::new());
            st.set_running(false);
        }
    }
}

/// Dispatch the diagnosed fix for `chain_index`, then re-sync that chain's live
/// runtime so the change is heard at once. Errors surface as a toast on the
/// main window.
fn apply_fix(
    session: &ProjectSession,
    chain_index: i32,
    main_weak: &Weak<AppWindow>,
    toast_timer: &Rc<slint::Timer>,
) {
    let result = apply_fix_inner(session, chain_index, |chain_id| {
        crate::runtime_sync_policy::request_chain_sync(session, chain_id)
    });
    if let Err(err) = result {
        if let Some(main_win) = main_weak.upgrade() {
            set_status_error(&main_win, toast_timer, &err.to_string());
        }
    }
}

/// Windowless core: dispatch the fix, then re-sync the chain's live runtime via
/// `sync_runtime` so the change is audible at once.
///
/// #808: this used to stop right after `dispatch`. The command mutated the
/// project model but nothing re-synced the runtime, so a doctor fix changed no
/// sound — and, while monitoring a DI (a dedicated pre-render, #717/#771),
/// NOTHING changed until a block toggle happened to re-arm it. Every other
/// param surface (`block_parameter_wiring`, `block_editor_persist`) syncs after
/// dispatch; the doctor's apply is on the same footing.
fn apply_fix_inner(
    session: &ProjectSession,
    chain_index: i32,
    sync_runtime: impl FnOnce(&domain::ids::ChainId) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let Some(chain) = chain_id_at(session, chain_index) else {
        return Ok(());
    };
    session
        .dispatcher
        .dispatch(Command::ToneDoctor(ToneDoctorCommand::ApplyToneDoctorFix {
            chain: chain.clone(),
        }))
        .map_err(|e| anyhow::anyhow!(e))?;
    sync_runtime(&chain)
}

#[cfg(test)]
#[path = "tone_doctor_compact_wiring_tests.rs"]
mod tests;

/// Wire the compact window's Tone Doctor run/apply for its single `chain_index`.
pub(crate) fn wire(
    compact_win: &CompactChainViewWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    project_runtime: ProjectRuntime,
    chain_index: i32,
    main_weak: Weak<AppWindow>,
    toast_timer: Rc<slint::Timer>,
) {
    crate::tone_doctor_events::register_compact(compact_win.as_weak());
    {
        let st = compact_win.global::<ToneDoctorState>();
        st.set_genre_options(genre_options(""));
        let weak = compact_win.as_weak();
        st.on_genre_query(move |q| {
            if let Some(w) = weak.upgrade() {
                w.global::<ToneDoctorState>()
                    .set_genre_options(genre_options(q.as_str()));
            }
        });
    }
    {
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
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
            // Re-registered per run so a device change (new runtime) is picked
            // up without restarting the app.
            attach_live_input(session, &project_runtime);
            start_run(&st, session, chain_index);
        });
    }
    {
        let project_session = project_session;
        compact_win.on_tone_doctor_apply(move |_ci| {
            let sb = project_session.borrow();
            let Some(session) = sb.as_ref() else {
                return;
            };
            apply_fix(session, chain_index, &main_weak, &toast_timer);
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
    let main_weak = window.as_weak();
    crate::tone_doctor_events::register_main(window.as_weak());
    {
        let st = window.global::<ToneDoctorState>();
        st.set_genre_options(genre_options(""));
        let weak = window.as_weak();
        st.on_genre_query(move |q| {
            if let Some(w) = weak.upgrade() {
                w.global::<ToneDoctorState>()
                    .set_genre_options(genre_options(q.as_str()));
            }
        });
    }
    {
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
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
            attach_live_input(session, &project_runtime);
            start_run(&st, session, ci);
        });
    }
    {
        let project_session = project_session;
        window.on_tone_doctor_apply(move |ci| {
            let sb = project_session.borrow();
            let Some(session) = sb.as_ref() else {
                return;
            };
            apply_fix(session, ci, &main_weak, &toast_timer);
        });
    }
}
