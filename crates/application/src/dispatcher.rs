//! `CommandDispatcher` trait — the single abstraction over the command bus.
//!
//! **Spec reference:** `docs/superpowers/specs/2026-04-23-command-dispatch-architecture-design.md`
//! — "Shared Architecture / Types".
//!
//! ## Send + Sync deferral
//!
//! The spec declares `CommandDispatcher: Send + Sync`. This trait intentionally
//! does NOT impose those bounds for Phase 1 because `LocalDispatcher` holds
//! `Rc<RefCell<ApplicationSession>>`, which is not `Send`. The bounds will be
//! added to `RemoteDispatcher` (Phase 2) which will use `Arc<Mutex<...>>`
//! internally. At that point a blanket impl or a second `RemoteCommandDispatcher`
//! supertrait will restore the `Send + Sync` contract for cross-thread callers.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use anyhow::Result;

use domain::ids::ChainId;
use engine::DiPcm;
use project::rig::RigProject;

use crate::command::Command;
use crate::di_loader::DiLoopSource;
use crate::event::Event;
use crate::local_dispatcher::ToneDoctorInput;
use crate::selection_state::SelectionState;

/// The single abstraction every consumer of the command bus uses.
///
/// Implementations:
/// - [`crate::local_dispatcher::LocalDispatcher`] — Phase 1, in-process.
/// - `RemoteDispatcher` — Phase 2, serialises commands over gRPC.
pub trait CommandDispatcher {
    /// Dispatch a command and return the resulting events.
    ///
    /// Errors are returned via `anyhow::Result` so implementations can surface
    /// domain errors (invalid chain index, validation failure, runtime error)
    /// without panicking.
    fn dispatch(&self, cmd: Command) -> Result<Vec<Event>>;

    /// #693: drain results of commands whose heavy work ran on its own
    /// task (e.g. the DI-loop WAV decode), apply them to dispatcher
    /// state, and return the completion events — same shape observers
    /// get from a synchronous dispatch. Called from the frontend poll
    /// tick. Default: nothing pending.
    fn poll_async_results(&self) -> Vec<Event> {
        Vec::new()
    }

    /// Engine sample rate the dispatcher last saw. `0` until the audio
    /// runtime reports one.
    fn engine_sr(&self) -> u32 {
        0
    }

    /// Shared UI selection state. Every implementation owns one — the
    /// selection is frontend state, not engine state.
    fn selection_state(&self) -> Arc<RwLock<SelectionState>>;

    /// Immutable copy of one chain, or `None` when the id is unknown.
    fn chain_snapshot(&self, _chain: &ChainId) -> Option<project::chain::Chain> {
        None
    }

    /// Decoded DI loop bound to a chain, when the implementation holds one.
    fn di_loop_for_chain(&self, _chain: &ChainId) -> Option<Arc<DiPcm>> {
        None
    }

    /// Where a chain's DI loop came from (file, looper, …).
    fn di_loop_source_for_chain(&self, _chain: &ChainId) -> Option<DiLoopSource> {
        None
    }

    /// Last Tone Doctor run for a chain, already serialized. `{}` when the
    /// implementation has never run one.
    fn tone_report_json(&self, _chain: &ChainId) -> String {
        "{}".to_string()
    }

    // --- session attach: local setup, no-op by default ---
    fn attach_rig(&self, _rig: Rc<RefCell<RigProject>>) {}
    fn attach_presets_path(&self, _path: PathBuf) {}
    fn attach_project_path(&self, _path: PathBuf) {}
    fn attach_config_path(&self, _path: Option<PathBuf>) {}
    /// Returns the chains whose resolved rate changed.
    fn attach_engine_sr(&self, _sr: u32) -> Vec<ChainId> {
        Vec::new()
    }
    /// #791: register how the Tone Doctor reaches a chain's live input. Only
    /// the adapter that owns the audio runtime can supply one, so a transport
    /// that does not own audio keeps the default no-op.
    fn attach_tone_doctor_input(&self, _provider: ToneDoctorInput) {}
}

#[cfg(test)]
#[path = "dispatcher_object_safety_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "dispatcher_tone_doctor_attach_tests.rs"]
mod tone_doctor_attach_tests;
