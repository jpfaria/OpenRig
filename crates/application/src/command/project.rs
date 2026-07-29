//! Project-scoped commands: session lifecycle (save/load/create/close), the
//! project's display name, the recent-projects list, and rig edit capture.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Every state change scoped to the project session as a whole.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ProjectCommand {
    // ── Project lifecycle ─────────────────────────────────────────────────────
    /// Save the project to its current path (or trigger save-as dialog).
    ///
    /// File I/O happens in the adapter before this command is dispatched. The
    /// dispatcher emits `ProjectSaved` to notify subscribers.
    SaveProject,

    /// Load a project from disk, replacing the current session.
    ///
    /// The adapter performs YAML parsing and constructs the `Project` before
    /// dispatching. The dispatcher replaces the shared project handle contents
    /// with the provided project and emits `ProjectLoaded { path }`.
    /// `path` is carried only for the event payload (not for I/O).
    LoadProject {
        project: project::project::Project,
        path: PathBuf,
    },

    /// Create a new project with the given name, replacing the current session.
    ///
    /// The adapter constructs the new empty `Project` before dispatching. The
    /// dispatcher replaces the shared project handle and emits `ProjectCreated`.
    CreateProject { project: project::project::Project },

    /// #436 E: close the current project (back to launcher). Was
    /// GUI-only (stop runtime + drop session in a wiring closure).
    /// `SaveProject` precedent: the adapter tears down the runtime/
    /// session; the dispatcher records the intent and signals
    /// `Event::ProjectClosed`.
    CloseProject,

    // ── Project settings ──────────────────────────────────────────────────────
    /// Update the project's display name.
    UpdateProjectName { name: String },

    // ── Recent projects ───────────────────────────────────────────────────────
    /// #436 (sweep): register/refresh a recent-projects entry. Was
    /// GUI-only (`register_recent_project` + `save_app_config` in
    /// open/save closures). `SaveProject` precedent: the adapter
    /// persists app-config; the dispatcher records the intent and
    /// signals `Event::RecentProjectRegistered`.
    RegisterRecentProject { path: PathBuf, name: String },

    /// #436 F: remove an entry from the recent-projects list (persisted
    /// app-config preference). Was GUI-only (`save_app_config` in a
    /// wiring closure). Now a Command so MIDI/MCP can request it too.
    /// `SaveProject` precedent: the adapter performs the persistence;
    /// the dispatcher records the intent and signals it via
    /// `Event::RecentProjectRemoved`.
    RemoveRecentProject { index: usize },

    /// #436 (sweep): mark a recent-projects entry invalid (failed open).
    /// Same precedent; signals `Event::RecentProjectInvalidated`.
    MarkRecentProjectInvalid { path: PathBuf, reason: String },

    // ── Rig ───────────────────────────────────────────────────────────────────
    /// #436: capture pending edits on the projected synthetic chains
    /// back into the rig. The GUI save path used to call
    /// `sync_synthetic_into_rig` by hand (model mutation in the UI);
    /// it now dispatches this so the dispatcher owns the mutation.
    CaptureRigEdits,
}
