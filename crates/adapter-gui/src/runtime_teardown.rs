//! Responsibility: stops the project audio runtime.
//! The two ways a rig STOPS — the same seam as `runtime_lifecycle`, split off
//! by concern (and because that file had reached its line cap).
//!
//! * [`stop_project_runtime`] — the whole rig goes: every stream this frontend
//!   has open is dropped.
//! * [`remove_live_chain_runtime`] — ONE chain goes, and the rest keep
//!   sounding.
//!
//! Both are owners, not wiring: nothing here is reachable from a UI callback
//! any more. `ProjectCommand::StopProjectRuntime` / `CloseProject` and
//! `ChainCommand::RemoveChain` reach them through
//! [`application::runtime_control::RuntimeControl`], so a footswitch, an MCP
//! client and the on-screen button stop the same audio (#127).
//!
//! Both end by re-publishing the engine sample rate, because a teardown is
//! exactly when the rate a consumer reads goes stale (#127/Task 11).

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::ChainId;
use infra_cpal::ProjectRuntimeController;

use crate::runtime_lifecycle::sync_engine_sr_from_runtime;
use crate::state::ProjectSession;

/// Drop the active controller, and tell the session's dispatcher the streams
/// are gone.
///
/// `session` is `None` when the project has already been closed — there is
/// then no dispatcher left to tell, but the streams must still go.
///
/// #127: the engine rate is part of the teardown. It was only ever pushed
/// forward, so a stopped 44.1 kHz rig left the dispatcher reporting 44 100 and
/// every consumer of the live rate — the block editor's EQ curve first among
/// them — kept drawing against a device that was no longer open. Resetting it
/// here means "nothing running" reads as the reference rate again, which is
/// what it is (issue #723's sanctioned no-device value, never a live-path
/// assumption).
pub(crate) fn stop_project_runtime(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    session: Option<&ProjectSession>,
) {
    if let Some(mut runtime) = project_runtime.borrow_mut().take() {
        runtime.stop();
    }
    if let Some(session) = session {
        sync_engine_sr_from_runtime(project_runtime, session.dispatcher.as_ref());
    }
}

/// Drop one chain from the live graph — the chain-delete teardown.
///
/// #127: this is the THIRD way a rig stops, and the one that used to leak a
/// dead rate. Deleting the last chain removed its streams but left the
/// controller alive, so nothing ever re-synced and every reader of the live
/// rate — the block editor's EQ curve first — kept the rate of a device that
/// was no longer open. It now mirrors `sync_live_chain_runtime`'s own
/// teardown: with nothing left running (no chain, no pending activation, no
/// armed DI — `is_running` counts all three, so a DI playing without any chain
/// keeps its controller, #808) the controller goes, and the engine rate is
/// re-synced either way. Two chains in, one deleted, the survivor's rate is
/// re-pushed unchanged.
///
/// **Isolation (invariant #4).** Only `chain_id`'s streams are touched. This
/// is NOT `sync_live_chain_runtime` for a chain that is gone: that sequence
/// validates the whole project first, so an unrelated invalid chain would
/// abort the teardown and leave a deleted chain sounding.
pub(crate) fn remove_live_chain_runtime(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    session: Option<&ProjectSession>,
    chain_id: &ChainId,
) {
    {
        let mut borrow = project_runtime.borrow_mut();
        if let Some(runtime) = borrow.as_mut() {
            runtime.remove_chain(chain_id);
            if !runtime.is_running() {
                *borrow = None;
            }
        }
    }
    if let Some(session) = session {
        sync_engine_sr_from_runtime(project_runtime, session.dispatcher.as_ref());
    }
}
