//! Responsibility: applies a chosen folder to the session that is running.

use crate::state::ProjectSession;
use application::command::{Command, PluginCommand, SettingsCommand};
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use infra_filesystem::AppConfig;
use rfd::FileDialog;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use super::paths_overrides::{
    apply_evaluations_override, apply_plugins_override, apply_presets_override,
};

/// Open a native folder picker and return the chosen directory (or
/// `None` if the user cancelled). Extracted so tests can verify the
/// downstream `persist + dispatch` path independently of the dialog
/// (the real dialog never runs in CI).
pub(crate) fn pick_folder_dialog() -> Option<PathBuf> {
    FileDialog::new().pick_folder()
}

/// Persist the new presets-path override into `config.yaml` and, when
/// a project session is loaded, dispatch `SettingsCommand::SetPresetsPath` so
/// the event fans out on the bus.
pub(crate) fn apply_presets_path(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    app_config: &Rc<RefCell<AppConfig>>,
    path: Option<PathBuf>,
) {
    if let Err(e) = apply_presets_override(&mut app_config.borrow_mut(), path.clone()) {
        log::warn!("[paths] failed to persist presets-path into config.yaml: {e}");
        return;
    }
    let session = project_session.borrow();
    let Some(session) = session.as_ref() else {
        return;
    };
    if let Err(e) = session
        .dispatcher
        .dispatch(Command::Settings(SettingsCommand::SetPresetsPath { path }))
    {
        log::warn!("[paths] Command::SetPresetsPath failed: {e}");
    }
}

/// Same as [`apply_presets_path`] but for the plugins override.
pub(crate) fn apply_plugins_path(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    app_config: &Rc<RefCell<AppConfig>>,
    path: Option<PathBuf>,
) {
    if let Err(e) = apply_plugins_override(&mut app_config.borrow_mut(), path.clone()) {
        log::warn!("[paths] failed to persist plugins-path into config.yaml: {e}");
        return;
    }
    let session = project_session.borrow();
    let Some(session) = session.as_ref() else {
        return;
    };
    if let Err(e) = session
        .dispatcher
        .dispatch(Command::Settings(SettingsCommand::SetPluginsPath { path }))
    {
        log::warn!("[paths] Command::SetPluginsPath failed: {e}");
    }
}

/// #582: same persist+dispatch pattern for the evaluations directory.
pub(crate) fn apply_evaluations_path(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    app_config: &Rc<RefCell<AppConfig>>,
    path: Option<PathBuf>,
) {
    if let Err(e) = apply_evaluations_override(&mut app_config.borrow_mut(), path.clone()) {
        log::warn!("[paths] failed to persist evaluations-path into config.yaml: {e}");
        return;
    }
    let session = project_session.borrow();
    let Some(session) = session.as_ref() else {
        return;
    };
    if let Err(e) =
        session
            .dispatcher
            .dispatch(Command::Settings(SettingsCommand::SetEvaluationsPath {
                path,
            }))
    {
        log::warn!("[paths] Command::SetEvaluationsPath failed: {e}");
    }
}

/// #561: dispatch `PluginCommand::ReloadPluginCatalog` and return a
/// human-readable summary of the new totals (or an error message
/// suitable for the status text). Both `install` and `install_secondary`
/// share this helper so the success/failure path is one place.
///
/// When no project session is attached we still dispatch through a
/// fresh in-process dispatcher snapshot — the catalog is process-wide
/// state, not project-scoped. This mirrors the boot path
/// (`init_many`), which runs before any project is loaded.
pub(crate) fn run_reload_plugin_catalog(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
) -> String {
    // Use the session's dispatcher when available so other listeners
    // (publishing fan-out) see the event; fall back to a one-shot
    // local dispatcher when no project is loaded (still triggers the
    // registry reload because the handler reaches the same process-
    // wide `plugin_loader::registry`).
    let events_result: anyhow::Result<Vec<Event>> = {
        let borrow = project_session.borrow();
        if let Some(session) = borrow.as_ref() {
            session
                .dispatcher
                .dispatch(Command::Plugin(PluginCommand::ReloadPluginCatalog))
        } else {
            drop(borrow);
            // No project session — run the side-effect directly via a
            // throwaway LocalDispatcher tied to an empty project. The
            // registry is process-wide so the reload still takes
            // effect for any future project session.
            let project = Rc::new(std::cell::RefCell::new(project::project::Project {
                name: None,
                device_settings: Vec::new(),
                chains: Vec::new(),
                midi: None,
            }));
            application::local_dispatcher::LocalDispatcher::new(project)
                .dispatch(Command::Plugin(PluginCommand::ReloadPluginCatalog))
        }
    };
    match events_result {
        Ok(events) => events
            .iter()
            .find_map(|e| match e {
                Event::PluginCatalogReloaded {
                    native_count,
                    disk_count,
                    total_count,
                } => Some(format!(
                    "{total_count} plugin(s) loaded ({native_count} native, {disk_count} disk)"
                )),
                _ => None,
            })
            .unwrap_or_else(|| "plugin catalog reloaded".to_string()),
        Err(e) => {
            log::warn!("[paths] Command::ReloadPluginCatalog failed: {e}");
            format!("reload failed: {e}")
        }
    }
}
