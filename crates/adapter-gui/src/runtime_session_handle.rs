//! #127: the open session, as seen from inside a command handler.
//!
//! `GuiRuntimeControl`'s doors need the whole [`ProjectSession`], but the
//! control cannot hold the app's `Rc<RefCell<Option<ProjectSession>>>`: every
//! GUI callback holds that cell `borrow_mut()` while it dispatches, so
//! borrowing it from a handler would panic. The session's own fields are cheap
//! shared handles, so they are mirrored instead — with the dispatcher held
//! **weakly**, because the dispatcher OWNS the control and an `Rc` back to it
//! would be a reference cycle that leaks the session (project data, DI loop
//! PCM, recorded loops) on every project switch.
//!
//! No audio backend is named here on purpose: this is the session's shape, not
//! the runtime's.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use application::dispatcher::CommandDispatcher;
use domain::io_binding::IoBinding;
use project::project::Project;
use project::rig::RigProject;

use crate::state::ProjectSession;

pub(crate) struct SessionHandle {
    project: Rc<RefCell<Project>>,
    dispatcher: Weak<dyn CommandDispatcher>,
    project_path: Option<PathBuf>,
    config_path: Option<PathBuf>,
    presets_path: PathBuf,
    rig: Option<Rc<RefCell<RigProject>>>,
    io_bindings: Rc<RefCell<Vec<IoBinding>>>,
}

impl SessionHandle {
    pub(crate) fn mirror(session: &ProjectSession) -> Self {
        Self {
            project: Rc::clone(&session.project),
            dispatcher: Rc::downgrade(&session.dispatcher),
            project_path: session.project_path.clone(),
            config_path: session.config_path.clone(),
            presets_path: session.presets_path.clone(),
            rig: session.rig.clone(),
            io_bindings: Rc::clone(&session.io_bindings),
        }
    }

    /// Rebuild the session the sync helpers take. `None` once the project has
    /// been closed — and then there is no runtime left to sync either.
    pub(crate) fn session(&self) -> Option<ProjectSession> {
        Some(ProjectSession {
            project: Rc::clone(&self.project),
            dispatcher: self.dispatcher.upgrade()?,
            project_path: self.project_path.clone(),
            config_path: self.config_path.clone(),
            presets_path: self.presets_path.clone(),
            rig: self.rig.clone(),
            io_bindings: Rc::clone(&self.io_bindings),
        })
    }
}
