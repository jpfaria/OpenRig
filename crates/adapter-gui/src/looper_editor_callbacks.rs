//! #826 — the waveform editor's callbacks: open it for a loop, and dispatch
//! the trim / crop / cut it asks for.
//!
//! Its own module (not `looper_callbacks`) because it is its own overlay with
//! its own read: the panel reads transport state, the editor reads the loop's
//! ENVELOPE. Both go through `LiveSource`, so the samples stay on the audio
//! side and MCP sees the same picture.
//!
//! The Slint side is a dispatcher and nothing else: it hands back the two
//! selection ratios, and the frames they mean are worked out HERE, against the
//! length the reading reports.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, LooperAction, LooperCommand};
use application::live_source::LiveSource;
use application::looper_edit::{playhead_ratio, EditOutcome, LoopEdit, LoopEditKind};
use domain::ids::ChainId;
use engine::LooperState;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::project_ops::sync_project_dirty;
use crate::state::ProjectSession;
use crate::{AppWindow, LoopEditKind as LoopEditKind_slint, LooperEditor};

type Session = Rc<RefCell<Option<ProjectSession>>>;
type Live = Rc<dyn LiveSource>;

/// What the editor needs to raise the project's dirty flag after an edit.
///
/// Without it the disquete stays grey and the edited loop is never written:
/// the audio lives in the store, and only a SAVE puts it on disk. Cloned into
/// each closure, exactly as the panel's callbacks do it.
#[derive(Clone)]
pub(crate) struct EditorDirtyCtx {
    pub window: slint::Weak<AppWindow>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub auto_save: bool,
}

/// Mark the project dirty after an edit, so the loop's new audio reaches disk
/// on the next save (or right away when auto-save is on).
fn mark_dirty(session: &ProjectSession, dirty: &EditorDirtyCtx) {
    if let Some(window) = dirty.window.upgrade() {
        sync_project_dirty(
            &window,
            session,
            &dirty.saved_project_snapshot,
            &dirty.project_dirty,
            dirty.auto_save,
        );
    }
}

/// How often the playhead is refreshed while the loop runs — the same ~15 Hz
/// the panel's meters use (#715): fast enough to read as motion, slow enough
/// that the poll never competes with the audio worker for cache.
const PLAYHEAD_TICK_MS: u64 = 66;

/// How many bars the waveform draws. Fixed: the view is a fixed-width overlay,
/// so a per-loop count would only make short loops look chunkier.
const WAVEFORM_BUCKETS: usize = 256;

/// Read the loop through the live seam and push it into the editor's global —
/// the peaks, and what the undo/redo buttons enable on. Called when the editor
/// opens and again after every edit, so the view always draws what the store
/// actually holds.
fn refresh_editor(window: &AppWindow, live: &Live, chain: &ChainId, uid: u64) -> Option<usize> {
    let reading = live.chain_loop_edit(chain, uid, WAVEFORM_BUCKETS)?;
    let editor = window.global::<LooperEditor>();
    editor.set_peaks(ModelRc::new(VecModel::from(reading.peaks)));
    editor.set_length_label(reading.length_label.into());
    editor.set_playing(reading.playing);
    editor.set_can_undo(reading.can_undo);
    editor.set_can_redo(reading.can_redo);
    Some(reading.len_frames)
}

/// The Slint enum, as the core spells it. A pure rename — what a selection
/// MEANS (clamping, ordering, the frames it covers) belongs to
/// `LoopEdit::from_ratios`, never to this frontend.
fn edit_kind(kind: LoopEditKind_slint) -> LoopEditKind {
    match kind {
        LoopEditKind_slint::Trim => LoopEditKind::Trim,
        LoopEditKind_slint::Crop => LoopEditKind::Crop,
        LoopEditKind_slint::Cut => LoopEditKind::Cut,
        LoopEditKind_slint::Fit => LoopEditKind::Fit,
    }
}

fn chain_id_at(session: &ProjectSession, index: i32) -> Option<ChainId> {
    let project = session.project.borrow();
    project.chains.get(index as usize).map(|c| c.id.clone())
}

pub(crate) fn wire_looper_editor_callbacks(
    window: &AppWindow,
    session: &Session,
    live: &Live,
    dirty_ctx: &EditorDirtyCtx,
) {
    // ── the playhead ────────────────────────────────────────────────────
    //
    // Its own tick, not the meter poll's: that poll never sees the window, and
    // this one is idle unless the editor is OPEN. It reads only the transport
    // (wait-free atomics, through the same seam MCP reads) — the loop's PCM is
    // never re-exported per frame.
    let playhead_timer = {
        let session = session.clone();
        let live = live.clone();
        let window_weak = window.as_weak();
        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(PLAYHEAD_TICK_MS),
            move || {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                let editor = window.global::<LooperEditor>();
                if !editor.get_open() {
                    return;
                }
                let session_borrow = session.borrow();
                let Some(chain) = session_borrow
                    .as_ref()
                    .and_then(|s| chain_id_at(s, editor.get_chain_index()))
                else {
                    return;
                };
                let uid = editor.get_uid() as u64;
                let Some(Ok((statuses, _rate))) = live.chain_loopers(&chain) else {
                    return;
                };
                let Some(status) = statuses.iter().find(|s| s.uid == uid) else {
                    return;
                };
                editor.set_playing(matches!(
                    status.state,
                    LooperState::Playing | LooperState::Overdubbing
                ));
                editor.set_playhead(playhead_ratio(status.position_frames, status.len_frames));
            },
        );
        timer
    };

    // ── open ────────────────────────────────────────────────────────────
    {
        let session = session.clone();
        let live = live.clone();
        let window_weak = window.as_weak();
        // The timer is owned by this callback, so it lives exactly as long as
        // the window does and is dropped with it.
        let _playhead_timer = playhead_timer;
        window.on_looper_edit(move |index, uid| {
            let _keep_ticking = &_playhead_timer;
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let session_borrow = session.borrow();
            let Some(chain) = session_borrow.as_ref().and_then(|s| chain_id_at(s, index)) else {
                return;
            };
            if refresh_editor(&window, &live, &chain, uid as u64).is_none() {
                // Nothing recorded (or no store hosted): opening an editor over
                // an empty loop would show a flat line and refuse every button.
                log::debug!("no material to edit on loop {uid} of chain {}", chain.0);
                return;
            }
            let editor = window.global::<LooperEditor>();
            editor.set_chain_index(index);
            editor.set_uid(uid);
            editor.set_sel_start(0.0);
            editor.set_sel_end(1.0);
            editor.set_status_code(0);
            editor.set_open(true);
        });
    }

    // ── the edits ───────────────────────────────────────────────────────
    {
        let session = session.clone();
        let live = live.clone();
        let window_weak = window.as_weak();
        let dirty_ctx = dirty_ctx.clone();
        window.on_looper_edit_apply(move |index, uid, kind, from, to| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let session_borrow = session.borrow();
            let Some(s) = session_borrow.as_ref() else {
                return;
            };
            let Some(chain) = chain_id_at(s, index) else {
                return;
            };
            let uid = uid as u64;
            // The length the ratios mean is the loop as it stands RIGHT NOW —
            // re-read, never remembered from when the editor opened.
            let Some(len_frames) = live
                .chain_loop_edit(&chain, uid, WAVEFORM_BUCKETS)
                .map(|r| r.len_frames)
            else {
                return;
            };
            let edit = LoopEdit::from_ratios(edit_kind(kind), len_frames, from, to);
            let dispatched = s
                .dispatcher
                .dispatch(Command::Looper(LooperCommand::EditChainLooperAudio {
                    chain: chain.clone(),
                    looper: uid,
                    edit,
                }));
            if let Err(err) = &dispatched {
                log::warn!("loop edit refused: {err}");
            }
            // The loop may be a different length now: reset the selection to
            // the whole take rather than keep ratios of a loop that no longer
            // exists, and re-read what the store actually holds.
            let editor = window.global::<LooperEditor>();
            editor.set_sel_start(0.0);
            editor.set_sel_end(1.0);
            let after = refresh_editor(&window, &live, &chain, uid);
            let outcome = EditOutcome::of(
                len_frames,
                if dispatched.is_ok() { after } else { None },
            );
            editor.set_status_code(outcome.code());
            // The loop's audio is runtime state: only a SAVE writes it to disk,
            // and the save only happens if the project knows it is dirty.
            if outcome == EditOutcome::Applied {
                mark_dirty(s, &dirty_ctx);
            }
        });
    }

    // ── the transport ───────────────────────────────────────────────────
    //
    // The SAME command the row's play button dispatches: one button, one
    // meaning, and `PlayStop` is resolved against the store's own state so the
    // editor can never disagree with the panel about what the loop is doing.
    {
        let session = session.clone();
        let live = live.clone();
        let window_weak = window.as_weak();
        window.on_looper_edit_play_stop(move |index, uid| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let session_borrow = session.borrow();
            let Some(s) = session_borrow.as_ref() else {
                return;
            };
            let Some(chain) = chain_id_at(s, index) else {
                return;
            };
            let uid = uid as u64;
            if let Err(err) =
                s.dispatcher
                    .dispatch(Command::Looper(LooperCommand::SetChainLooperTransport {
                        chain: chain.clone(),
                        looper: uid,
                        action: LooperAction::PlayStop,
                    }))
            {
                log::warn!("loop transport from the editor failed: {err}");
                return;
            }
            refresh_editor(&window, &live, &chain, uid);
        });
    }

    // ── undo / redo ─────────────────────────────────────────────────────
    macro_rules! step {
        ($setter:ident, $command:ident) => {{
            let session = session.clone();
            let live = live.clone();
            let window_weak = window.as_weak();
            let dirty_ctx = dirty_ctx.clone();
            window.$setter(move |index, uid| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                let session_borrow = session.borrow();
                let Some(s) = session_borrow.as_ref() else {
                    return;
                };
                let Some(chain) = chain_id_at(s, index) else {
                    return;
                };
                let uid = uid as u64;
                if let Err(err) =
                    s.dispatcher
                        .dispatch(Command::Looper(LooperCommand::$command {
                            chain: chain.clone(),
                            looper: uid,
                        }))
                {
                    log::warn!("loop edit history step failed: {err}");
                    return;
                }
                let editor = window.global::<LooperEditor>();
                editor.set_sel_start(0.0);
                editor.set_sel_end(1.0);
                refresh_editor(&window, &live, &chain, uid);
                mark_dirty(s, &dirty_ctx);
            });
        }};
    }

    step!(on_looper_edit_undo, UndoChainLooperEdit);
    step!(on_looper_edit_redo, RedoChainLooperEdit);
}
