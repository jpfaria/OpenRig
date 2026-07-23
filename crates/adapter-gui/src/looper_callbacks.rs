//! #323 — the looper panel's callbacks: every one dispatches a `Command` and
//! applies the result to the chain's runtimes in the same turn.
//!
//! The #614 rule: a dispatch alone is dead. The dispatcher owns the project
//! side (which loopers exist, their parameters); `looper_wiring` turns the
//! emitted events into audio-thread ops. Neither step is optional.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, LooperAction, LooperParam};
use application::dispatcher::CommandDispatcher;
use domain::ids::ChainId;
use infra_cpal::ProjectRuntimeController;
use project::chain::LooperSpeed;

use crate::looper_wiring::apply_looper_event;
use crate::state::ProjectSession;
use crate::AppWindow;

type Session = Rc<RefCell<Option<ProjectSession>>>;
type Runtime = Rc<RefCell<Option<ProjectRuntimeController>>>;

fn chain_id_at(session: &ProjectSession, index: i32) -> Option<ChainId> {
    let project = session.project.borrow();
    project.chains.get(index as usize).map(|c| c.id.clone())
}

/// Dispatch `cmd` and apply every event it produced to the chain's runtimes.
fn dispatch_and_apply(session: &ProjectSession, runtime: &Runtime, cmd: Command) {
    match session.dispatcher.dispatch(cmd) {
        Ok(events) => {
            let runtime_borrow = runtime.borrow();
            if let Some(controller) = runtime_borrow.as_ref() {
                for event in &events {
                    apply_looper_event(controller, event);
                }
            }
        }
        Err(err) => log::warn!("looper command failed: {err}"),
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
pub(crate) fn wire_looper_callbacks(window: &AppWindow, session: &Session, runtime: &Runtime) {
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
        window.on_looper_add(move |index| {
            with_chain!(session, index, |s: &ProjectSession, chain: ChainId| {
                dispatch_and_apply(s, &runtime, Command::AddChainLooper { chain });
            });
        });
    }
    {
        let session = session.clone();
        let runtime = runtime.clone();
        window.on_looper_remove(move |index, uid| {
            with_chain!(session, index, |s: &ProjectSession, chain: ChainId| {
                dispatch_and_apply(
                    s,
                    &runtime,
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
            window.$setter(move |index, uid| {
                with_chain!(session, index, |s: &ProjectSession, chain: ChainId| {
                    dispatch_and_apply(
                        s,
                        &runtime,
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
            window.$setter(move |index, uid, value| {
                with_chain!(session, index, |s: &ProjectSession, chain: ChainId| {
                    dispatch_and_apply(
                        s,
                        &runtime,
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
}
