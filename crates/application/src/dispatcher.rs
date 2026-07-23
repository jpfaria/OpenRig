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

use anyhow::Result;

use crate::command::Command;
use crate::event::Event;

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
}
