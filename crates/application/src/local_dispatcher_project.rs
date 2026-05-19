//! Project lifecycle/settings handler (file-per-feature; #436 split).
//! Behaviour byte-identical to the original inline arm — pure move.

use anyhow::Result;

use crate::command::Command;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// Project lifecycle + settings commands.
    pub(crate) fn handle_project(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            // ── Project lifecycle ─────────────────────────────────────────────
            // File I/O happens in the adapter before dispatch. The dispatcher
            // signals the completion via events only.
            Command::SaveProject => Ok(vec![Event::ProjectSaved]),

            Command::LoadProject { project, path: _ } => {
                // Replace the shared project data in-place so all Rc::clone
                // holders (adapter-gui's ProjectSession) see the updated state.
                *self.project.borrow_mut() = project;
                Ok(vec![Event::ProjectLoaded, Event::ProjectMutated])
            }

            Command::CreateProject { project } => {
                *self.project.borrow_mut() = project;
                Ok(vec![Event::ProjectCreated, Event::ProjectMutated])
            }

            // ── Project settings ──────────────────────────────────────────────
            Command::UpdateProjectName { name } => {
                let trimmed = name.trim().to_string();
                self.project.borrow_mut().name = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                };
                Ok(vec![Event::ProjectMutated])
            }

            Command::SaveAudioSettings { device_settings } => {
                self.project.borrow_mut().device_settings = device_settings;
                Ok(vec![Event::AudioSettingsSaved])
            }
            other => unreachable!("handle_project received non-project command: {other:?}"),
        }
    }
}
