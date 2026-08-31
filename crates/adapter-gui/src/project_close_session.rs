//! Responsibility: clears what a closed project leaves behind in this session.
//!
//! Split out of `back_to_launcher_wiring` (#913) so the teardown is reachable
//! by a test: hiding windows and flipping view flags is screen logic and stays
//! in the callback, but dropping the session, forgetting the saved snapshot and
//! emptying the chain rows is what actually has to happen — a project that is
//! "closed" while its session still exists keeps answering as if it were open.

use std::cell::RefCell;
use std::rc::Rc;

use domain::AudioDeviceDescriptor;
use project::project::Project;
use slint::VecModel;

use crate::project_view::replace_project_chains;
use crate::state::ProjectSession;
use crate::ProjectChainItem;
use application::command::{Command, ProjectCommand};

/// Dispatch `CloseProject`, then drop everything this session held for it.
///
/// The command goes first and while the session still exists: #127 made it the
/// step that STOPS THE RIG, so a project closed over MCP/gRPC goes silent too
/// — before that the teardown lived in the GUI callback and every stream kept
/// sounding. Dropping the session afterwards is adapter-side.
pub(crate) fn close_session(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    project_chains: &Rc<VecModel<ProjectChainItem>>,
    saved_project_snapshot: &Rc<RefCell<Option<String>>>,
    input_chain_devices: &[AudioDeviceDescriptor],
    output_chain_devices: &[AudioDeviceDescriptor],
) {
    if let Some(session) = project_session.borrow().as_ref() {
        if let Err(e) = session
            .dispatcher
            .dispatch(Command::Project(ProjectCommand::CloseProject))
        {
            log::warn!("[back-to-launcher] Command::CloseProject falhou: {e}");
        }
    }
    *project_session.borrow_mut() = None;
    *saved_project_snapshot.borrow_mut() = None;
    replace_project_chains(
        project_chains,
        &Project {
            name: None,
            device_settings: Vec::new(),
            chains: Vec::new(),
            midi: None,
        },
        input_chain_devices,
        output_chain_devices,
        &[],
    );
}

#[cfg(test)]
#[path = "project_close_session_tests.rs"]
mod tests;
