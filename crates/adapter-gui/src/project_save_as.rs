//! Responsibility: re-points a session at the file Save As chose.
//!
//! Split out of `project_file_dialog_wiring` (#913). Opening the dialog is
//! screen work; re-binding everything that derives from the project's path is
//! not, and every binding here has been a bug when it was missed:
//!
//! * `presets_path` follows the project's folder, so presets land next to the
//!   file the user just chose;
//! * the dispatcher is re-attached, or #555 writes the YAML to the OLD path;
//! * the runtime seam is re-attached, or a cold start after Save As restores
//!   the project's loops from the old path (#127).

use std::path::PathBuf;

use crate::runtime_lifecycle::RuntimeAttach;
use crate::state::ProjectSession;

/// Point `session` at `path` and re-attach everything derived from it.
pub(crate) fn bind_project_path(
    session: &mut ProjectSession,
    path: PathBuf,
    runtime_attach: &RuntimeAttach,
) {
    session.project_path = Some(path.clone());
    session.config_path = Some(crate::project_ops::resolve_project_config_path(&path));
    session.presets_path = path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("presets");
    session.dispatcher.attach_project_path(path);
    session
        .dispatcher
        .attach_config_path(session.config_path.clone());
    session
        .dispatcher
        .attach_presets_path(session.presets_path.clone());
    runtime_attach.to_session(session);
}

#[cfg(test)]
#[path = "project_save_as_tests.rs"]
mod tests;
