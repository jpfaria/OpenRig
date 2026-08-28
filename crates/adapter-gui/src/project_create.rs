//! Responsibility: builds a new, unsaved project into this session.
//!
//! Split out of `project_file_dialog_wiring` (#913). Switching the view is
//! screen work; the sequence is not. The previous rig stops first, the audio
//! seam is wired to the NEW session before anything can dispatch against it
//! (#127), and the clean snapshot is deliberately left EMPTY: a project that
//! exists only in memory is dirty from its first frame, so closing without
//! saving prompts instead of discarding silently.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use application::command::{Command, ProjectCommand};
use domain::AudioDeviceDescriptor;
use slint::VecModel;

use crate::project_file_dialog_wiring::stop_the_previous_rig;
use crate::project_ops::create_new_project_session;
use crate::project_view::replace_project_chains;
use crate::runtime_lifecycle::RuntimeAttach;
use crate::state::ProjectSession;
use crate::ProjectChainItem;

/// Create a project named `name` and install it as this session's.
///
/// The caller has already refused an empty name — the launcher says so before
/// getting here.
#[allow(clippy::too_many_arguments)]
pub(crate) fn create_project(
    name: &str,
    default_config_path: &Path,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    project_chains: &Rc<VecModel<ProjectChainItem>>,
    runtime_attach: &RuntimeAttach,
    saved_project_snapshot: &Rc<RefCell<Option<String>>>,
    input_chain_devices: &[AudioDeviceDescriptor],
    output_chain_devices: &[AudioDeviceDescriptor],
) {
    stop_the_previous_rig(project_session);
    let session = create_new_project_session(default_config_path);
    // #436: creating is business — the command goes on the bus so MCP/MIDI see
    // `Event::ProjectCreated`; building the session is adapter-side.
    {
        let project = session.project.borrow().clone();
        if let Err(e) = session
            .dispatcher
            .dispatch(Command::Project(ProjectCommand::CreateProject { project }))
        {
            log::warn!("[new-project] Command::CreateProject failed: {e}");
        }
    }
    let _ = session
        .dispatcher
        .dispatch(Command::Project(ProjectCommand::UpdateProjectName {
            name: name.to_string(),
        }));
    replace_project_chains(
        project_chains,
        &session.project.borrow(),
        input_chain_devices,
        output_chain_devices,
        &[],
    );
    // #127: the seam is wired BEFORE the session is installed, so a
    // runtime-control command issued before the first chain sync still reaches
    // the audio.
    runtime_attach.to_session(&session);
    *project_session.borrow_mut() = Some(session);
    // No snapshot on purpose: an in-memory project is dirty from its first
    // frame, so closing it without saving prompts.
    *saved_project_snapshot.borrow_mut() = None;
}

#[cfg(test)]
#[path = "project_create_tests.rs"]
mod tests;
