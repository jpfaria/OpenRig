//! Responsibility: wires the paths section.
//! System / Paths section wiring (#513). Two rows — Presets and
//! Plugins — each backed by a "Choose…" callback that opens an
//! `rfd::FileDialog::pick_folder()` and a "Reset" callback that clears
//! the override (so the OS default wins again). Both callbacks
//! persist the choice into `config.yaml` immediately (via
//! `FilesystemStorage::save_*_path`) AND dispatch
//! `SettingsCommand::SetPresetsPath` / `SettingsCommand::SetPluginsPath` so the event
//! fans out on the bus (MCP/gRPC parity). Pattern matches
//! `midi_devices`: persist locally + dispatch the Command, identical
//! to `SaveMidiDevices`.
//!
//! When no project session is loaded the dispatch is skipped (mirrors
//! `midi_devices::install`): persistence still happens so the choice
//! survives even before a project is opened.
//!
//! #607: each Choose…/Reset also mirrors the override into the shared
//! in-memory `AppConfig`, not only `config.yaml`. Lifecycle events
//! (project-open / register-recent) re-persist the whole in-memory
//! snapshot via `save_app_config(&app_config.borrow())`; without the
//! mirror, that whole-config save would clobber a just-picked override
//! back to its startup value (the user-visible bug: evaluations folder
//! reverting to default after reopening the project).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global};

use infra_filesystem::AppConfig;

pub(crate) use super::paths_apply::{
    apply_evaluations_path, apply_plugins_path, apply_presets_path, pick_folder_dialog,
    run_reload_plugin_catalog,
};
pub use super::paths_overrides::{
    apply_evaluations_override, apply_plugins_override, apply_presets_override,
};
pub use super::paths_seed::{seed_initial, seed_initial_secondary};
use crate::state::ProjectSession;
use crate::{AppWindow, ProjectSettingsWindow};

/// Install the Paths section callbacks on the primary `AppWindow`.
/// Each Choose… opens the native folder dialog, persists into
/// `config.yaml`, and updates the Slint property so the UI reflects
/// the new value immediately. Each Reset clears the override.
pub fn install(
    win: &AppWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    app_config: Rc<RefCell<AppConfig>>,
) {
    // ── presets / Choose ────────────────────────────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    crate::SettingsBridge::get(win).on_pick_presets_path(move || {
        let Some(path) = pick_folder_dialog() else {
            return;
        };
        apply_presets_path(&session, &config, Some(path.clone()));
        if let Some(w) = win_weak.upgrade() {
            crate::SettingsBridge::get(&w)
                .set_presets_path(path.to_string_lossy().into_owned().into());
        }
    });

    // ── presets / Reset ─────────────────────────────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    crate::SettingsBridge::get(win).on_reset_presets_path(move || {
        apply_presets_path(&session, &config, None);
        if let Some(w) = win_weak.upgrade() {
            crate::SettingsBridge::get(&w).set_presets_path(slint::SharedString::default());
        }
    });

    // ── plugins / Choose ────────────────────────────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    crate::SettingsBridge::get(win).on_pick_plugins_path(move || {
        let Some(path) = pick_folder_dialog() else {
            return;
        };
        apply_plugins_path(&session, &config, Some(path.clone()));
        if let Some(w) = win_weak.upgrade() {
            crate::SettingsBridge::get(&w)
                .set_plugins_path(path.to_string_lossy().into_owned().into());
        }
    });

    // ── plugins / Reset ─────────────────────────────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    crate::SettingsBridge::get(win).on_reset_plugins_path(move || {
        apply_plugins_path(&session, &config, None);
        if let Some(w) = win_weak.upgrade() {
            crate::SettingsBridge::get(&w).set_plugins_path(slint::SharedString::default());
        }
    });

    // ── evaluations / Choose (#582) ─────────────────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    crate::SettingsBridge::get(win).on_pick_evaluations_path(move || {
        let Some(path) = pick_folder_dialog() else {
            return;
        };
        apply_evaluations_path(&session, &config, Some(path.clone()));
        if let Some(w) = win_weak.upgrade() {
            crate::SettingsBridge::get(&w)
                .set_evaluations_path(path.to_string_lossy().into_owned().into());
        }
    });

    // ── evaluations / Reset (#582) ──────────────────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    crate::SettingsBridge::get(win).on_reset_evaluations_path(move || {
        apply_evaluations_path(&session, &config, None);
        if let Some(w) = win_weak.upgrade() {
            crate::SettingsBridge::get(&w).set_evaluations_path(slint::SharedString::default());
        }
    });

    // ── #561 reload plugin catalog ──────────────────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    crate::SettingsBridge::get(win).on_reload_plugin_catalog(move || {
        let status = run_reload_plugin_catalog(&session);
        if let Some(w) = win_weak.upgrade() {
            crate::SettingsBridge::get(&w).set_plugin_catalog_status(status.into());
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
    app_config: Rc<RefCell<AppConfig>>,
) {
    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    crate::SettingsBridge::get(win).on_pick_presets_path(move || {
        let Some(path) = pick_folder_dialog() else {
            return;
        };
        apply_presets_path(&session, &config, Some(path.clone()));
        if let Some(w) = win_weak.upgrade() {
            crate::SettingsBridge::get(&w)
                .set_presets_path(path.to_string_lossy().into_owned().into());
        }
    });

    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    crate::SettingsBridge::get(win).on_reset_presets_path(move || {
        apply_presets_path(&session, &config, None);
        if let Some(w) = win_weak.upgrade() {
            crate::SettingsBridge::get(&w).set_presets_path(slint::SharedString::default());
        }
    });

    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    crate::SettingsBridge::get(win).on_pick_plugins_path(move || {
        let Some(path) = pick_folder_dialog() else {
            return;
        };
        apply_plugins_path(&session, &config, Some(path.clone()));
        if let Some(w) = win_weak.upgrade() {
            crate::SettingsBridge::get(&w)
                .set_plugins_path(path.to_string_lossy().into_owned().into());
        }
    });

    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    crate::SettingsBridge::get(win).on_reset_plugins_path(move || {
        apply_plugins_path(&session, &config, None);
        if let Some(w) = win_weak.upgrade() {
            crate::SettingsBridge::get(&w).set_plugins_path(slint::SharedString::default());
        }
    });

    // ── evaluations (#582) — secondary window ───────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    crate::SettingsBridge::get(win).on_pick_evaluations_path(move || {
        let Some(path) = pick_folder_dialog() else {
            return;
        };
        apply_evaluations_path(&session, &config, Some(path.clone()));
        if let Some(w) = win_weak.upgrade() {
            crate::SettingsBridge::get(&w)
                .set_evaluations_path(path.to_string_lossy().into_owned().into());
        }
    });

    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    crate::SettingsBridge::get(win).on_reset_evaluations_path(move || {
        apply_evaluations_path(&session, &config, None);
        if let Some(w) = win_weak.upgrade() {
            crate::SettingsBridge::get(&w).set_evaluations_path(slint::SharedString::default());
        }
    });

    // ── #561 reload plugin catalog (secondary window) ───────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    crate::SettingsBridge::get(win).on_reload_plugin_catalog(move || {
        let status = run_reload_plugin_catalog(&session);
        if let Some(w) = win_weak.upgrade() {
            crate::SettingsBridge::get(&w).set_plugin_catalog_status(status.into());
        }
    });
}
