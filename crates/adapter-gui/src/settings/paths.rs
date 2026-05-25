//! System / Paths section wiring (#513). Two rows — Presets and
//! Plugins — each backed by a "Choose…" callback that opens an
//! `rfd::FileDialog::pick_folder()` and a "Reset" callback that clears
//! the override (so the OS default wins again). Both callbacks
//! persist the choice into `config.yaml` immediately (via
//! `FilesystemStorage::save_*_path`) AND dispatch
//! `Command::SetPresetsPath` / `Command::SetPluginsPath` so the event
//! fans out on the bus (MCP/gRPC parity). Pattern matches
//! `midi_devices`: persist locally + dispatch the Command, identical
//! to `SaveMidiDevices`.
//!
//! When no project session is loaded the dispatch is skipped (mirrors
//! `midi_devices::install`): persistence still happens so the choice
//! survives even before a project is opened.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use rfd::FileDialog;
use slint::ComponentHandle;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use infra_filesystem::FilesystemStorage;

use crate::state::ProjectSession;
use crate::{AppWindow, ProjectSettingsWindow};

/// Open a native folder picker and return the chosen directory (or
/// `None` if the user cancelled). Extracted so tests can verify the
/// downstream `persist + dispatch` path independently of the dialog
/// (the real dialog never runs in CI).
fn pick_folder_dialog() -> Option<PathBuf> {
    FileDialog::new().pick_folder()
}

/// Persist the new presets-path override into `config.yaml` and, when
/// a project session is loaded, dispatch `Command::SetPresetsPath` so
/// the event fans out on the bus.
fn apply_presets_path(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    path: Option<PathBuf>,
) {
    if let Err(e) = FilesystemStorage::save_presets_path(path.clone()) {
        log::warn!("[paths] failed to persist presets-path into config.yaml: {e}");
        return;
    }
    let session = project_session.borrow();
    let Some(session) = session.as_ref() else {
        return;
    };
    if let Err(e) = session
        .dispatcher
        .dispatch(Command::SetPresetsPath { path })
    {
        log::warn!("[paths] Command::SetPresetsPath failed: {e}");
    }
}

/// Same as [`apply_presets_path`] but for the plugins override.
fn apply_plugins_path(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    path: Option<PathBuf>,
) {
    if let Err(e) = FilesystemStorage::save_plugins_path(path.clone()) {
        log::warn!("[paths] failed to persist plugins-path into config.yaml: {e}");
        return;
    }
    let session = project_session.borrow();
    let Some(session) = session.as_ref() else {
        return;
    };
    if let Err(e) = session
        .dispatcher
        .dispatch(Command::SetPluginsPath { path })
    {
        log::warn!("[paths] Command::SetPluginsPath failed: {e}");
    }
}

/// Install the Paths section callbacks on the primary `AppWindow`.
/// Each Choose… opens the native folder dialog, persists into
/// `config.yaml`, and updates the Slint property so the UI reflects
/// the new value immediately. Each Reset clears the override.
pub fn install(win: &AppWindow, project_session: Rc<RefCell<Option<ProjectSession>>>) {
    // ── presets / Choose ────────────────────────────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    win.on_pick_presets_path(move || {
        let Some(path) = pick_folder_dialog() else {
            return;
        };
        apply_presets_path(&session, Some(path.clone()));
        if let Some(w) = win_weak.upgrade() {
            w.set_presets_path(path.to_string_lossy().into_owned().into());
        }
    });

    // ── presets / Reset ─────────────────────────────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    win.on_reset_presets_path(move || {
        apply_presets_path(&session, None);
        if let Some(w) = win_weak.upgrade() {
            w.set_presets_path(slint::SharedString::default());
        }
    });

    // ── plugins / Choose ────────────────────────────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    win.on_pick_plugins_path(move || {
        let Some(path) = pick_folder_dialog() else {
            return;
        };
        apply_plugins_path(&session, Some(path.clone()));
        if let Some(w) = win_weak.upgrade() {
            w.set_plugins_path(path.to_string_lossy().into_owned().into());
        }
    });

    // ── plugins / Reset ─────────────────────────────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    win.on_reset_plugins_path(move || {
        apply_plugins_path(&session, None);
        if let Some(w) = win_weak.upgrade() {
            w.set_plugins_path(slint::SharedString::default());
        }
    });
}

/// Mirror of [`install`] for the standalone `ProjectSettingsWindow`.
/// Same Rc state so edits made in either surface share the same
/// `apply_*` function — one persistence write and one Command dispatch
/// per user action.
pub fn install_secondary(
    win: &ProjectSettingsWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
) {
    let win_weak = win.as_weak();
    let session = project_session.clone();
    win.on_pick_presets_path(move || {
        let Some(path) = pick_folder_dialog() else {
            return;
        };
        apply_presets_path(&session, Some(path.clone()));
        if let Some(w) = win_weak.upgrade() {
            w.set_presets_path(path.to_string_lossy().into_owned().into());
        }
    });

    let win_weak = win.as_weak();
    let session = project_session.clone();
    win.on_reset_presets_path(move || {
        apply_presets_path(&session, None);
        if let Some(w) = win_weak.upgrade() {
            w.set_presets_path(slint::SharedString::default());
        }
    });

    let win_weak = win.as_weak();
    let session = project_session.clone();
    win.on_pick_plugins_path(move || {
        let Some(path) = pick_folder_dialog() else {
            return;
        };
        apply_plugins_path(&session, Some(path.clone()));
        if let Some(w) = win_weak.upgrade() {
            w.set_plugins_path(path.to_string_lossy().into_owned().into());
        }
    });

    let win_weak = win.as_weak();
    let session = project_session.clone();
    win.on_reset_plugins_path(move || {
        apply_plugins_path(&session, None);
        if let Some(w) = win_weak.upgrade() {
            w.set_plugins_path(slint::SharedString::default());
        }
    });
}

/// Seed the initial `presets-path` / `plugins-path` Slint properties
/// from the persisted `AppConfig.paths` snapshot so the Settings
/// screen renders the user's current choice on first open. Called
/// once at startup from `desktop_app::setup`.
pub fn seed_initial(win: &AppWindow) {
    let config = FilesystemStorage::load_app_config().unwrap_or_default();
    let presets = config
        .paths
        .presets_path
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let plugins = config
        .paths
        .plugins_path
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    win.set_presets_path(presets.into());
    win.set_plugins_path(plugins.into());
}

/// Mirror of [`seed_initial`] for the secondary `ProjectSettingsWindow`.
pub fn seed_initial_secondary(win: &ProjectSettingsWindow) {
    let config = FilesystemStorage::load_app_config().unwrap_or_default();
    let presets = config
        .paths
        .presets_path
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let plugins = config
        .paths
        .plugins_path
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    win.set_presets_path(presets.into());
    win.set_plugins_path(plugins.into());
}
