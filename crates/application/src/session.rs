//! Minimal session state held by the `LocalDispatcher`.
//!
//! `ApplicationSession` is intentionally narrow: it carries only what the
//! dispatcher needs to execute `Command`s. It does NOT pull `adapter-gui`
//! into the `application` crate's dependency tree.
//!
//! `adapter-gui` will construct an `ApplicationSession` from its own
//! `ProjectSession` before dispatching a command, and merge the result back
//! after the dispatcher returns. That translation layer is a Task N+1 concern.

use project::project::Project;

/// Minimal session state required by [`crate::local_dispatcher::LocalDispatcher`].
pub struct ApplicationSession {
    pub project: Project,
}
