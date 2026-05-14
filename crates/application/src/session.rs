//! Minimal session state held by the `LocalDispatcher`.
//!
//! `ApplicationSession` is intentionally narrow: it carries only what the
//! dispatcher needs to execute `Command`s. It does NOT pull `adapter-gui`
//! into the `application` crate's dependency tree.
//!
//! `adapter-gui` wraps its `Project` in an `Rc<RefCell<Project>>` and shares
//! the same handle with the `LocalDispatcher` via
//! `ApplicationSession::from_shared`. Both sides then see the SAME mutations
//! with no extra sync step.

use std::cell::RefCell;
use std::rc::Rc;

use project::project::Project;

/// Minimal session state required by [`crate::local_dispatcher::LocalDispatcher`].
///
/// The `project` field is a shared, interior-mutable handle so that
/// `adapter-gui`'s `ProjectSession` and the dispatcher can both operate on
/// the same `Project` without duplicating or copying it.
pub struct ApplicationSession {
    pub project: Rc<RefCell<Project>>,
}

impl ApplicationSession {
    /// Create a new session that owns a fresh `Project`.
    pub fn new(project: Project) -> Self {
        Self {
            project: Rc::new(RefCell::new(project)),
        }
    }

    /// Wrap an existing shared `Project` handle.
    ///
    /// Use this when the caller already holds an `Rc<RefCell<Project>>` (e.g.
    /// `adapter-gui`'s `ProjectSession`) and wants the dispatcher to operate on
    /// the same data.
    pub fn from_shared(project: Rc<RefCell<Project>>) -> Self {
        Self { project }
    }
}
