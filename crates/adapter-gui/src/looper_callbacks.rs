//! #323 — the looper panel's callbacks: every one dispatches a `Command` and
//! refreshes the chain-card's looper rows in the same turn.
//!
//! #127: this module used to apply the runtime half itself — it created the
//! audio runtime, pushed each emitted event into the controller's looper store
//! and reconciled the loop's playback stream, all in the callback that had
//! just dispatched. The dispatcher does all of that now, through
//! `application::runtime_control::RuntimeControl`, so a footswitch and an MCP
//! client reach exactly what this panel reaches. What is left here is a
//! frontend's job and nothing else: dispatch, then re-read the live state
//! through `LiveSource` and redraw.
//!
//! The row rebuild stays: without it a chain with no live stream never updated
//! (the meter timer only visits chains with a running stream), so the user
//! re-clicked Add until the config hit the 8-looper cap with an empty panel.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, LooperAction, LooperCommand, LooperParam};
use application::live_source::LiveSource;
use domain::ids::ChainId;
use engine::LooperStatus;
use project::chain::LooperSpeed;
use slint::{ComponentHandle, VecModel};

use crate::project_ops::sync_project_dirty;
use crate::state::ProjectSession;
use crate::{AppWindow, ProjectChainItem};

type Session = Rc<RefCell<Option<ProjectSession>>>;
/// The panel's read seam: the loopers' live transport state, and the rate they
/// are counted at. The same seam MCP reads (`QueryKind::ChainLoopers`), so the
/// panel and a remote client can never disagree about what a loop is doing.
type Live = Rc<dyn LiveSource>;
type Chains = Rc<VecModel<ProjectChainItem>>;

/// Everything `dispatch_and_apply` needs to mark the project dirty after a
/// looper edit — the disquete stays grey otherwise, and the user loses loops
/// because Save never prompts (the looper path was the only mutation flow that
/// forgot this, unlike every block/insert wiring). Cloned into each closure.
#[derive(Clone)]
struct LooperDirtyCtx {
    window: slint::Weak<AppWindow>,
    saved_project_snapshot: Rc<RefCell<Option<String>>>,
    project_dirty: Rc<RefCell<bool>>,
    auto_save: bool,
}

fn chain_id_at(session: &ProjectSession, index: i32) -> Option<ChainId> {
    let project = session.project.borrow();
    project.chains.get(index as usize).map(|c| c.id.clone())
}

/// Rebuild the chain-card's looper rows from the current project + the live
/// reading, so an add / remove / param / transport shows at once.
///
/// A reading is a FINISHED one: transport states and the rate they were
/// counted at, never PCM. With no live source hosted (the rig is stopped) the
/// rows come from the persisted config, at no sample rate at all — a rate for
/// a device nobody opened would be a fiction (#723).
fn refresh_row(session: &ProjectSession, live: &Live, chains: &Chains, index: i32) {
    let project = session.project.borrow();
    let Some(chain) = project.chains.get(index as usize) else {
        return;
    };
    let registry = session.io_bindings.borrow();
    let preset_ids = crate::looper_view::chain_preset_ids(chain, session.rig.as_deref());
    let reading = live.chain_loopers(&chain.id).and_then(Result::ok);
    let (statuses, rate) = match &reading {
        Some((statuses, rate)) => (statuses.as_slice(), Some(*rate)),
        None => (&[] as &[LooperStatus], None),
    };
    crate::looper_view::write_chain_looper_row(
        chains,
        index as usize,
        chain,
        statuses,
        rate,
        &registry,
        &preset_ids,
    );
}

/// Dispatch `cmd`, then refresh the row so the panel reflects the change
/// immediately.
///
/// The runtime half — claiming the store slot, starting the audio runtime the
/// looper needs (#808), applying the transport and reconciling the loop's
/// isolated playback stream — happens INSIDE the dispatch now. By the time
/// this returns, the store and the streams already say what the row is about
/// to draw.
fn dispatch_and_apply(
    session: &ProjectSession,
    live: &Live,
    chains: &Chains,
    index: i32,
    cmd: Command,
    dirty: &LooperDirtyCtx,
) {
    match session.dispatcher.dispatch(cmd) {
        Ok(_) => {
            refresh_row(session, live, chains, index);
            // Mark the project dirty (the disquete) so the user is prompted to
            // save — a looper add/record/param edit changes the persisted chain,
            // and without this the loop is silently lost on close (#323).
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
        Err(err) => log::warn!("looper command failed: {err}"),
    }
}

/// #323 phase 2: when the user starts a FRESH recording (the loop holds no
/// material yet), link it to the chain's currently-active preset so it keeps
/// that tone even after the chain switches preset to solo. An overdub (the loop
/// already has audio) never relinks. No-op for a non-rig chain or when no
/// preset is active.
fn link_active_preset_on_fresh_record(
    session: &ProjectSession,
    live: &Live,
    chain: &ChainId,
    uid: u64,
) {
    let Some(preset_id) = session
        .rig
        .as_deref()
        .and_then(|r| crate::chain_preset_wiring::active_preset_id(chain, &r.borrow()))
    else {
        return;
    };
    // Fresh = no recorded material yet (Empty, or no live entry at all).
    let fresh = live
        .chain_loopers(chain)
        .and_then(Result::ok)
        .map(|(statuses, _)| {
            matches!(
                statuses.iter().find(|s| s.uid == uid).map(|s| s.state),
                None | Some(engine::LooperState::Empty)
            )
        })
        .unwrap_or(true);
    if fresh {
        let _ = session
            .dispatcher
            .dispatch(Command::Looper(LooperCommand::SetChainLooperPreset {
                chain: chain.clone(),
                looper: uid,
                preset: Some(preset_id),
            }));
    }
}

fn speed_from_index(index: i32) -> LooperSpeed {
    match index {
        0 => LooperSpeed::Half,
        2 => LooperSpeed::Double,
        _ => LooperSpeed::Normal,
    }
}

/// Connect every looper callback of the app window.
pub(crate) fn wire_looper_callbacks(
    window: &AppWindow,
    session: &Session,
    live: &Live,
    chains: &Chains,
    saved_project_snapshot: &Rc<RefCell<Option<String>>>,
    project_dirty: &Rc<RefCell<bool>>,
    auto_save: bool,
) {
    let dirty_ctx = LooperDirtyCtx {
        window: window.as_weak(),
        saved_project_snapshot: saved_project_snapshot.clone(),
        project_dirty: project_dirty.clone(),
        auto_save,
    };
    macro_rules! with_chain {
        ($session:expr, $index:expr, $body:expr) => {{
            let session_borrow = $session.borrow();
            if let Some(session) = session_borrow.as_ref() {
                if let Some(chain) = chain_id_at(session, $index) {
                    #[allow(clippy::redundant_closure_call)]
                    $body(session, chain);
                }
            }
        }};
    }

    // ── add / remove ────────────────────────────────────────────────────
    {
        let session = session.clone();
        let live = live.clone();
        let chains = chains.clone();
        let dirty_ctx = dirty_ctx.clone();
        window.on_looper_add(move |index| {
            with_chain!(session, index, |s: &ProjectSession, chain: ChainId| {
                dispatch_and_apply(
                    s,
                    &live,
                    &chains,
                    index,
                    Command::Looper(LooperCommand::AddChainLooper { chain }),
                    &dirty_ctx,
                );
            });
        });
    }
    {
        let session = session.clone();
        let live = live.clone();
        let chains = chains.clone();
        let dirty_ctx = dirty_ctx.clone();
        window.on_looper_remove(move |index, uid| {
            with_chain!(session, index, |s: &ProjectSession, chain: ChainId| {
                dispatch_and_apply(
                    s,
                    &live,
                    &chains,
                    index,
                    Command::Looper(LooperCommand::RemoveChainLooper {
                        chain,
                        looper: uid as u64,
                    }),
                    &dirty_ctx,
                );
            });
        });
    }

    // ── transport ───────────────────────────────────────────────────────
    macro_rules! transport {
        ($setter:ident, $action:expr) => {{
            let session = session.clone();
            let live = live.clone();
            let chains = chains.clone();
            let dirty_ctx = dirty_ctx.clone();
            window.$setter(move |index, uid| {
                with_chain!(session, index, |s: &ProjectSession, chain: ChainId| {
                    dispatch_and_apply(
                        s,
                        &live,
                        &chains,
                        index,
                        Command::Looper(LooperCommand::SetChainLooperTransport {
                            chain,
                            looper: uid as u64,
                            action: $action,
                        }),
                        &dirty_ctx,
                    );
                });
            });
        }};
    }
    // RECORD is special (#323 phase 2): starting a FRESH recording links the
    // loop to the chain's currently-active preset, so it keeps that tone even
    // after the chain switches preset to solo. Overdubs (loop already has
    // material) never relink. Then the Record transport proceeds as usual.
    {
        let session = session.clone();
        let live = live.clone();
        let chains = chains.clone();
        let dirty_ctx = dirty_ctx.clone();
        window.on_looper_record(move |index, uid| {
            with_chain!(session, index, |s: &ProjectSession, chain: ChainId| {
                link_active_preset_on_fresh_record(s, &live, &chain, uid as u64);
                dispatch_and_apply(
                    s,
                    &live,
                    &chains,
                    index,
                    Command::Looper(LooperCommand::SetChainLooperTransport {
                        chain,
                        looper: uid as u64,
                        action: LooperAction::Record,
                    }),
                    &dirty_ctx,
                );
            });
        });
    }
    transport!(on_looper_undo, LooperAction::Undo);
    transport!(on_looper_redo, LooperAction::Redo);
    transport!(on_looper_clear, LooperAction::Clear);

    // Play/stop is one button: the PlayStop action is resolved against the
    // runtime by the wiring, so the GUI and a footswitch behave identically.
    transport!(on_looper_play_stop, LooperAction::PlayStop);

    // ── parameters ──────────────────────────────────────────────────────
    macro_rules! param {
        ($setter:ident, $make:expr) => {{
            let session = session.clone();
            let live = live.clone();
            let chains = chains.clone();
            let dirty_ctx = dirty_ctx.clone();
            window.$setter(move |index, uid, value| {
                with_chain!(session, index, |s: &ProjectSession, chain: ChainId| {
                    dispatch_and_apply(
                        s,
                        &live,
                        &chains,
                        index,
                        Command::Looper(LooperCommand::SetChainLooperParam {
                            chain,
                            looper: uid as u64,
                            param: $make(value),
                        }),
                        &dirty_ctx,
                    );
                });
            });
        }};
    }
    param!(on_looper_mix_changed, |v: i32| LooperParam::Mix(
        v as f32 / 100.0
    ));
    param!(on_looper_decay_changed, |v: i32| LooperParam::Decay(
        v as f32 / 100.0
    ));
    param!(on_looper_speed_picked, |v: i32| LooperParam::Speed(
        speed_from_index(v)
    ));
    param!(on_looper_reverse_toggled, |v: bool| LooperParam::Reverse(v));

    // ── input / output endpoint selection ───────────────────────────────
    // Picking the record input re-issues the looper's segment binding so REC
    // captures the chosen input; picking the output persists the choice.
    {
        let session = session.clone();
        let live = live.clone();
        let chains = chains.clone();
        let dirty_ctx = dirty_ctx.clone();
        window.on_looper_input_picked(move |index, uid, endpoint_index| {
            with_chain!(session, index, |s: &ProjectSession, chain: ChainId| {
                let registry = s.io_bindings.borrow();
                let input = s.project.borrow().chains.get(index as usize).and_then(|c| {
                    project::binding_discovery::input_endpoint_ref(
                        c,
                        &registry,
                        endpoint_index as usize,
                    )
                });
                drop(registry);
                dispatch_and_apply(
                    s,
                    &live,
                    &chains,
                    index,
                    Command::Looper(LooperCommand::SetChainLooperInput {
                        chain,
                        looper: uid as u64,
                        input,
                    }),
                    &dirty_ctx,
                );
            });
        });
    }
    {
        let session = session.clone();
        let live = live.clone();
        let chains = chains.clone();
        let dirty_ctx = dirty_ctx.clone();
        window.on_looper_output_picked(move |index, uid, endpoint_index| {
            with_chain!(session, index, |s: &ProjectSession, chain: ChainId| {
                let registry = s.io_bindings.borrow();
                let output = s.project.borrow().chains.get(index as usize).and_then(|c| {
                    project::binding_discovery::output_endpoint_ref(
                        c,
                        &registry,
                        endpoint_index as usize,
                    )
                });
                drop(registry);
                dispatch_and_apply(
                    s,
                    &live,
                    &chains,
                    index,
                    Command::Looper(LooperCommand::SetChainLooperOutput {
                        chain,
                        looper: uid as u64,
                        output,
                    }),
                    &dirty_ctx,
                );
            });
        });
    }
    // #323 phase 2: a preset was picked in the drawer's modal. Option 0 =
    // "follow the chain" (clear the link → None); k = the chain's bank slot k.
    // The linked-preset playback blocks re-resolve on the next meter tick.
    {
        let session = session.clone();
        let live = live.clone();
        let chains = chains.clone();
        let dirty_ctx = dirty_ctx.clone();
        window.on_looper_preset_picked(move |index, uid, option_index| {
            with_chain!(session, index, |s: &ProjectSession, chain: ChainId| {
                let preset = if option_index <= 0 {
                    None
                } else {
                    s.rig.as_deref().and_then(|r| {
                        crate::chain_preset_wiring::chain_preset_bank(&chain, &r.borrow())
                            .into_iter()
                            .nth((option_index - 1) as usize)
                            .map(|(id, _)| id)
                    })
                };
                dispatch_and_apply(
                    s,
                    &live,
                    &chains,
                    index,
                    Command::Looper(LooperCommand::SetChainLooperPreset {
                        chain,
                        looper: uid as u64,
                        preset,
                    }),
                    &dirty_ctx,
                );
            });
        });
    }
}
