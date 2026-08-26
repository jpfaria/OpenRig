//! Responsibility: reports the health the frontend polls for.
//! #127: the write half of the frontend's poll tick — installing finished
//! rebuilds and reconnecting a backend that died.
//!
//! Neither is a `Command`: nobody asks for either one. The tick services work
//! the frontend itself queued (#672), and the reconnect is automatic recovery
//! from a machine losing its audio host — no project state changes in either.
//! They are still WRITES to the runtime, so they live behind
//! [`RuntimeControl`] like every other write, and `desktop_app_polling` reaches
//! them through that seam instead of holding the backend.
//!
//! Unlike `GuiRuntimeControl` (which mirrors ONE session, the one that
//! dispatched) this control is built once, at startup, over the cells the app
//! shares — the timers outlive every project the user opens, so the session is
//! resolved per call.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Result;

use application::runtime_control::RuntimeControl;
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;

use crate::state::ProjectSession;

/// The poll tick's `RuntimeControl`. Holds the app-wide cells, never a
/// snapshot: a reconnect must re-sync the project that is open NOW.
struct PollingRuntimeControl {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    session: Rc<RefCell<Option<ProjectSession>>>,
}

impl RuntimeControl for PollingRuntimeControl {
    /// #323: the meter tick's looper reconcile, for ONE chain. The body lives
    /// in `runtime_loopers.rs` with the rest of the looper doors; the rig it
    /// resolves the linked presets against is the one open NOW, like the
    /// reconnect below.
    fn reconcile_chain_loopers(&self, chain: &Chain) {
        let session = self.session.borrow();
        let rig = session.as_ref().and_then(|s| s.rig.as_deref());
        crate::runtime_loopers::reconcile_chain_loopers(&self.runtime, chain, rig);
    }

    /// #672: swap in whatever the control worker finished building. Each
    /// queued build carries the (chain, group) it was requested for and lands
    /// in that slot alone; this only services the queue.
    fn apply_finished_rebuilds(&self) -> usize {
        match self.runtime.borrow_mut().as_mut() {
            Some(controller) => controller.poll_pending_rebuilds(),
            // Nothing hosted ⇒ nothing was ever queued. A tick never creates a
            // runtime: it is not a request to hear anything.
            None => 0,
        }
    }

    /// Tear the streams down and re-open them for the project that is open
    /// NOW — read from the shared cell, not from a session snapshot taken
    /// when the timers started.
    ///
    /// The project borrow is taken while the runtime is held mutably, exactly
    /// as the health timer took it before: the reconnect re-syncs the project
    /// it is handed, and re-reading it here is what makes "reconnect" mean the
    /// rig the user has loaded rather than the one they opened first.
    fn reconnect_audio(&self) -> Result<bool> {
        let mut runtime = self.runtime.borrow_mut();
        let Some(controller) = runtime.as_mut() else {
            return Ok(false);
        };
        let session = self.session.borrow();
        let Some(session) = session.as_ref() else {
            return Ok(false);
        };
        let project = session.project.borrow();
        controller.try_reconnect(&project)
    }
}

/// Build the poll tick's write seam over the app's shared handles. Called by
/// `desktop_app`, the module that allocates them.
pub(crate) fn polling_runtime_control(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    session: &Rc<RefCell<Option<ProjectSession>>>,
) -> Rc<dyn RuntimeControl> {
    Rc::new(PollingRuntimeControl {
        runtime: Rc::clone(runtime),
        session: Rc::clone(session),
    })
}

#[cfg(test)]
#[path = "runtime_health_tests.rs"]
mod tests;
