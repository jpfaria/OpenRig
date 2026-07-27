//! #323 — the looper panel's callbacks: every one dispatches a `Command`,
//! applies the result to the chain's runtimes, and refreshes the chain-card's
//! looper rows in the same turn.
//!
//! The #614 rule: a dispatch alone is dead. The dispatcher owns the project
//! side (which loopers exist, their parameters); `looper_wiring` turns the
//! emitted events into audio-thread ops; and the row rebuild gives the panel
//! immediate feedback — without it a chain with no live stream never updated
//! (the meter timer only visits chains with a running stream), so the user
//! re-clicked Add until the config hit the 8-looper cap with an empty panel.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, LooperAction, LooperParam};
use application::dispatcher::CommandDispatcher;
use domain::ids::ChainId;
use infra_cpal::ProjectRuntimeController;
use project::chain::LooperSpeed;
use slint::VecModel;

use crate::looper_wiring::apply_looper_event;
use crate::state::ProjectSession;
use crate::{AppWindow, ProjectChainItem};

type Session = Rc<RefCell<Option<ProjectSession>>>;
type Runtime = Rc<RefCell<Option<ProjectRuntimeController>>>;
type Chains = Rc<VecModel<ProjectChainItem>>;

fn chain_id_at(session: &ProjectSession, index: i32) -> Option<ChainId> {
    let project = session.project.borrow();
    project.chains.get(index as usize).map(|c| c.id.clone())
}

/// Rebuild the chain-card's looper rows from the current project + live
/// runtime, so an add / remove / param / transport shows at once. Reads the
/// live status only when the chain has a running stream; otherwise the rows
/// come from the persisted config (no fictional sample rate).
fn refresh_row(session: &ProjectSession, runtime: &Runtime, chains: &Chains, index: i32) {
    let project = session.project.borrow();
    let Some(chain) = project.chains.get(index as usize) else {
        return;
    };
    let registry = session.io_bindings.borrow();
    let runtime_borrow = runtime.borrow();
    match runtime_borrow.as_ref() {
        Some(controller) if !controller.runtimes_for_chain(&chain.id).is_empty() => {
            crate::looper_view::write_chain_looper_row(
                chains,
                index as usize,
                chain,
                &controller.chain_looper_statuses(&chain.id),
                Some(controller.sample_rate()),
                &registry,
            );
        }
        _ => crate::looper_view::write_chain_looper_row(
            chains,
            index as usize,
            chain,
            &[],
            None,
            &registry,
        ),
    }
}

/// Dispatch `cmd`, apply every event it produced to the chain's runtimes, then
/// refresh the row so the panel reflects the change immediately.
fn dispatch_and_apply(
    session: &ProjectSession,
    runtime: &Runtime,
    chains: &Chains,
    index: i32,
    cmd: Command,
) {
    eprintln!("[looper-probe] dispatch {cmd:?} (chain index {index})");
    match session.dispatcher.dispatch(cmd) {
        Ok(events) => {
            {
                let runtime_borrow = runtime.borrow();
                match runtime_borrow.as_ref() {
                    Some(controller) => {
                        for event in &events {
                            eprintln!(
                                "[looper-probe] apply {event:?} — runtimes={}",
                                event
                                    .chain()
                                    .map(|c| controller.runtimes_for_chain(c).len())
                                    .unwrap_or(0)
                            );
                            apply_looper_event(controller, event);
                        }
                    }
                    None => eprintln!("[looper-probe] NO controller — chain not started"),
                }
            }
            refresh_row(session, runtime, chains, index);
        }
        Err(err) => eprintln!("[looper-probe] looper command failed: {err}"),
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
    runtime: &Runtime,
    chains: &Chains,
) {
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
        let runtime = runtime.clone();
        let chains = chains.clone();
        window.on_looper_add(move |index| {
            with_chain!(session, index, |s: &ProjectSession, chain: ChainId| {
                dispatch_and_apply(
                    s,
                    &runtime,
                    &chains,
                    index,
                    Command::AddChainLooper { chain },
                );
            });
        });
    }
    {
        let session = session.clone();
        let runtime = runtime.clone();
        let chains = chains.clone();
        window.on_looper_remove(move |index, uid| {
            with_chain!(session, index, |s: &ProjectSession, chain: ChainId| {
                dispatch_and_apply(
                    s,
                    &runtime,
                    &chains,
                    index,
                    Command::RemoveChainLooper {
                        chain,
                        looper: uid as u64,
                    },
                );
            });
        });
    }

    // ── transport ───────────────────────────────────────────────────────
    macro_rules! transport {
        ($setter:ident, $action:expr) => {{
            let session = session.clone();
            let runtime = runtime.clone();
            let chains = chains.clone();
            window.$setter(move |index, uid| {
                with_chain!(session, index, |s: &ProjectSession, chain: ChainId| {
                    dispatch_and_apply(
                        s,
                        &runtime,
                        &chains,
                        index,
                        Command::SetChainLooperTransport {
                            chain,
                            looper: uid as u64,
                            action: $action,
                        },
                    );
                });
            });
        }};
    }
    transport!(on_looper_record, LooperAction::Record);
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
            let runtime = runtime.clone();
            let chains = chains.clone();
            window.$setter(move |index, uid, value| {
                with_chain!(session, index, |s: &ProjectSession, chain: ChainId| {
                    dispatch_and_apply(
                        s,
                        &runtime,
                        &chains,
                        index,
                        Command::SetChainLooperParam {
                            chain,
                            looper: uid as u64,
                            param: $make(value),
                        },
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
        let runtime = runtime.clone();
        let chains = chains.clone();
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
                    &runtime,
                    &chains,
                    index,
                    Command::SetChainLooperInput {
                        chain: chain.clone(),
                        looper: uid as u64,
                        input,
                    },
                );
                // Re-bind the looper to the chosen segment so REC captures it
                // (the bank keeps whatever was already recorded).
                let seg = endpoint_index.max(0) as usize;
                let uid = uid as u64;
                controller_op(&runtime, &chain, engine::LooperOp::Create { uid, seg });
            });
        });
    }
    {
        let session = session.clone();
        let runtime = runtime.clone();
        let chains = chains.clone();
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
                    &runtime,
                    &chains,
                    index,
                    Command::SetChainLooperOutput {
                        chain,
                        looper: uid as u64,
                        output,
                    },
                );
            });
        });
    }
}

/// Push a raw looper op to every runtime of a chain (used for the segment
/// re-bind on input change, which is not carried by an event).
fn controller_op(runtime: &Runtime, chain: &ChainId, op: engine::LooperOp) {
    let borrow = runtime.borrow();
    let Some(controller) = borrow.as_ref() else {
        return;
    };
    // The op carries no buffer, so a single instance is enough per runtime.
    let uid = match &op {
        engine::LooperOp::Create { uid, .. } => *uid,
        _ => return,
    };
    let seg = match &op {
        engine::LooperOp::Create { seg, .. } => *seg,
        _ => 0,
    };
    controller.push_chain_looper_op(chain, |_| Some(engine::LooperOp::Create { uid, seg }));
}
