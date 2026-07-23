//! Issue #693 — published state snapshot for API-style reads.
//!
//! Transports (MCP/gRPC) must serve reads like an HTTP API serves a
//! GET: concurrently, on their own task, never queued behind the
//! frontend thread or another caller. The frontend remains the single
//! WRITER (command order is user data), but after every dispatch it
//! publishes an immutable snapshot here; readers `latest()` it
//! lock-free (arc-swap) and serialize on their own thread.
//!
//! Consistency model: a snapshot reflects the state as of the last
//! dispatched command — cross-client reads are eventually consistent
//! within one swap (microseconds). Process-global on purpose: one app
//! instance owns exactly one project, same precedent as
//! `persist_worker`.

use std::sync::Arc;
use std::sync::OnceLock;

use arc_swap::ArcSwapOption;
use project::project::Project;
use project::rig::RigProject;

/// Immutable view of the dispatcher-owned state at one point in time.
pub struct StateSnapshot {
    pub project: Project,
    pub rig: Option<RigProject>,
}

fn cell() -> &'static ArcSwapOption<StateSnapshot> {
    static CELL: OnceLock<ArcSwapOption<StateSnapshot>> = OnceLock::new();
    CELL.get_or_init(ArcSwapOption::const_empty)
}

/// Publish a fresh snapshot (writer side — the dispatching thread).
pub fn publish(snapshot: StateSnapshot) {
    cell().store(Some(Arc::new(snapshot)));
}

/// Latest published snapshot, if any command has been dispatched yet.
/// Lock-free; any thread, any number of concurrent readers.
pub fn latest() -> Option<Arc<StateSnapshot>> {
    cell().load_full()
}
