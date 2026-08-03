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

use application::command::{Command, LooperCommand};
use application::live_source::LiveSource;
use application::looper_edit::{LoopEdit, LoopEditKind};
use domain::ids::ChainId;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::state::ProjectSession;
use crate::{AppWindow, LoopEditKind as LoopEditKind_slint, LooperEditor};

type Session = Rc<RefCell<Option<ProjectSession>>>;
type Live = Rc<dyn LiveSource>;

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
) {
    // ── open ────────────────────────────────────────────────────────────
    {
        let session = session.clone();
        let live = live.clone();
        let window_weak = window.as_weak();
        window.on_looper_edit(move |index, uid| {
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
            editor.set_open(true);
        });
    }

    // ── the edits ───────────────────────────────────────────────────────
    {
        let session = session.clone();
        let live = live.clone();
        let window_weak = window.as_weak();
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
            match s
                .dispatcher
                .dispatch(Command::Looper(LooperCommand::EditChainLooperAudio {
                    chain: chain.clone(),
                    looper: uid,
                    edit,
                })) {
                Ok(_) => {
                    // The loop is a different length now: reset the selection
                    // to the whole take rather than keep ratios of a loop that
                    // no longer exists.
                    let editor = window.global::<LooperEditor>();
                    editor.set_sel_start(0.0);
                    editor.set_sel_end(1.0);
                    refresh_editor(&window, &live, &chain, uid);
                }
                Err(err) => log::warn!("loop edit refused: {err}"),
            }
        });
    }

    // ── undo / redo ─────────────────────────────────────────────────────
    macro_rules! step {
        ($setter:ident, $command:ident) => {{
            let session = session.clone();
            let live = live.clone();
            let window_weak = window.as_weak();
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
            });
        }};
    }

    step!(on_looper_edit_undo, UndoChainLooperEdit);
    step!(on_looper_edit_redo, RedoChainLooperEdit);
}
