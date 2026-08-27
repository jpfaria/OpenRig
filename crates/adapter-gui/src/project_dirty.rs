//! Responsibility: tracks whether the open project has unsaved changes.

use crate::state::ProjectSession;
use crate::AppWindow;
use anyhow::Result;
use application::command::{Command, ProjectCommand};

/// The dirty-detection fingerprint. For a rig session the saved artifact
/// is the `.openrig` (the `RigProject`), so the fingerprint MUST include
/// it — switching preset/scene or editing sources often projects an
/// identical legacy `Project` (e.g. a scene with no overrides), so a
/// legacy-only snapshot would never flip dirty and Save would never be
/// offered ("cliquei numa scene e não deu opção de salvar"). Pure.
pub(crate) fn dirty_snapshot(
    project: &project::project::Project,
    rig: Option<&project::rig::RigProject>,
) -> Result<String> {
    let legacy = infra_yaml::serialize_project(project)?;
    match rig {
        Some(rig) => Ok(format!(
            "{legacy}\n---openrig---\n{}",
            infra_yaml::serialize_rig_project(rig)?
        )),
        None => Ok(legacy),
    }
}

pub(crate) fn project_session_snapshot(session: &ProjectSession) -> Result<String> {
    let rig = session.rig.as_ref().map(|r| r.borrow());
    dirty_snapshot(&session.project.borrow(), rig.as_deref())
}

pub(crate) fn set_project_dirty(
    window: &AppWindow,
    project_dirty: &std::rc::Rc<std::cell::RefCell<bool>>,
    dirty: bool,
) {
    *project_dirty.borrow_mut() = dirty;
    window.set_project_dirty(dirty);
}

#[track_caller]
pub(crate) fn sync_project_dirty(
    window: &AppWindow,
    session: &ProjectSession,
    saved_project_snapshot: &std::rc::Rc<std::cell::RefCell<Option<String>>>,
    project_dirty: &std::rc::Rc<std::cell::RefCell<bool>>,
    auto_save: bool,
) {
    if auto_save {
        if let Some(ref path) = session.project_path {
            // #555: auto-save goes through the dispatcher too — the
            // file writes live inside `ProjectCommand::SaveProject`. Keep the
            // local snapshot fingerprint up to date so the next
            // dirty-check is accurate.
            match session
                .dispatcher
                .dispatch(Command::Project(ProjectCommand::SaveProject))
            {
                Ok(_) => {
                    *saved_project_snapshot.borrow_mut() = project_session_snapshot(session).ok();
                    set_project_dirty(window, project_dirty, false);
                    log::debug!("auto-save: saved to {:?}", path);
                    return;
                }
                Err(e) => log::error!("auto-save failed: {e}"),
            }
        }
    }
    let dirty = match saved_project_snapshot.borrow().as_ref() {
        Some(saved_snapshot) => project_session_snapshot(session)
            .map(|current| current != *saved_snapshot)
            .unwrap_or(true),
        None => true,
    };
    set_project_dirty(window, project_dirty, dirty);
}

/// #555: test-only shim that dispatches `ProjectCommand::SaveProject` after
/// attaching the session's paths. Production callers go through
/// `dispatch(Command::Project(ProjectCommand::SaveProject))` directly —
/// this shim exists so the existing `project_ops_persistence_tests`
/// suite keeps exercising the end-to-end save path without each
/// test repeating the four attach + dispatch lines.
#[cfg(test)]
pub(crate) fn save_project_session(
    session: &ProjectSession,
    project_path: &std::path::Path,
) -> Result<()> {
    session
        .dispatcher
        .attach_project_path(project_path.to_path_buf());
    session
        .dispatcher
        .attach_presets_path(session.presets_path.clone());
    session
        .dispatcher
        .attach_config_path(session.config_path.clone());
    let result = session
        .dispatcher
        .dispatch(Command::Project(ProjectCommand::SaveProject))
        .map(|_| ());
    // #693: writes are queued to the persist worker; the round-trip
    // suites reload right after saving, so wait for durability here.
    application::persist_worker::flush();
    result
}

// `save_chain_blocks_to_preset` was moved to
// `application::local_dispatcher_preset::handle_chain_preset` in #555.
// The GUI now dispatches `ChainCommand::SaveChainPreset { chain, name }`
// and the dispatcher does the file write.
