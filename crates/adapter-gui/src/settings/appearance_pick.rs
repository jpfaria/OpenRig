//! Responsibility: keeps an Appearance pick beyond the window that made it.
//!
//! #398: a pick is kept in `config.yaml` — through the bus when a project is
//! open, so MCP reaches the same command, or directly from the launcher,
//! which has no dispatcher. The boot `AppConfig` snapshot is mirrored so a
//! wholesale re-save (recent projects, project open) never brings the old
//! scheme back.

use std::cell::RefCell;
use std::path::PathBuf;

use application::command::{Command, SettingsCommand};
use infra_filesystem::{AppConfig, Appearance, FilesystemStorage};

use crate::state::ProjectSession;

/// Mirrors the scheme into the boot `AppConfig` snapshot.
pub(crate) fn mirror(app_config: &RefCell<AppConfig>, appearance: Appearance) {
    app_config.borrow_mut().appearance = appearance;
}

/// On the bus when a project is open; straight to this machine's
/// `config.yaml` otherwise.
pub(crate) fn record(project_session: &RefCell<Option<ProjectSession>>, appearance: Appearance) {
    if let Some(session) = project_session.borrow().as_ref() {
        let command = Command::Settings(SettingsCommand::SetAppearance { appearance });
        if let Err(e) = session.dispatcher.dispatch(command) {
            log::warn!("[appearance] dispatch failed: {e}");
        }
        return;
    }
    match FilesystemStorage::app_config_path() {
        Ok(path) => save_at(path, appearance),
        Err(e) => log::error!("[appearance] resolve config path failed: {e}"),
    }
}

/// Writes only the scheme into `config_path` on the persist worker. The path
/// is bound by the caller, never re-resolved on the worker (#731).
pub(crate) fn save_at(config_path: PathBuf, appearance: Appearance) {
    application::persist_worker::run(move || {
        let keep = |config: &mut AppConfig| config.appearance = appearance;
        if let Err(e) = FilesystemStorage::update_app_config_at(&config_path, keep) {
            log::error!("failed to persist appearance: {e}");
        }
    });
}
