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
//!
//! #607: each Choose…/Reset also mirrors the override into the shared
//! in-memory `AppConfig`, not only `config.yaml`. Lifecycle events
//! (project-open / register-recent) re-persist the whole in-memory
//! snapshot via `save_app_config(&app_config.borrow())`; without the
//! mirror, that whole-config save would clobber a just-picked override
//! back to its startup value (the user-visible bug: evaluations folder
//! reverting to default after reopening the project).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use rfd::FileDialog;
use slint::ComponentHandle;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use infra_filesystem::{AppConfig, FilesystemStorage};

use crate::state::ProjectSession;
use crate::{AppWindow, ProjectSettingsWindow};

/// Open a native folder picker and return the chosen directory (or
/// `None` if the user cancelled). Extracted so tests can verify the
/// downstream `persist + dispatch` path independently of the dialog
/// (the real dialog never runs in CI).
fn pick_folder_dialog() -> Option<PathBuf> {
    FileDialog::new().pick_folder()
}

/// #607: Apply a **presets** path override — persist it into `config.yaml`
/// AND mirror it into the shared in-memory `AppConfig`. The mirror is the
/// fix: lifecycle events (project-open / register-recent) re-persist the
/// whole in-memory snapshot via `save_app_config(&app_config.borrow())`; if
/// the picker only wrote to disk, that whole-config save would clobber the
/// user's pick back to its startup value. Keeping the snapshot in lockstep
/// makes the override the single source of truth.
pub fn apply_presets_override(config: &mut AppConfig, path: Option<PathBuf>) -> anyhow::Result<()> {
    FilesystemStorage::save_presets_path(path.clone())?;
    config.paths.presets_path = path;
    Ok(())
}

/// #607: same persist + in-memory mirror for the **plugins** override.
pub fn apply_plugins_override(config: &mut AppConfig, path: Option<PathBuf>) -> anyhow::Result<()> {
    FilesystemStorage::save_plugins_path(path.clone())?;
    config.paths.plugins_path = path;
    Ok(())
}

/// #607: same persist + in-memory mirror for the **evaluations** override.
pub fn apply_evaluations_override(
    config: &mut AppConfig,
    path: Option<PathBuf>,
) -> anyhow::Result<()> {
    FilesystemStorage::save_evaluations_path(path.clone())?;
    config.paths.evaluations_path = path;
    Ok(())
}

/// Persist the new presets-path override into `config.yaml` and, when
/// a project session is loaded, dispatch `Command::SetPresetsPath` so
/// the event fans out on the bus.
fn apply_presets_path(
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
        .dispatch(Command::SetPresetsPath { path })
    {
        log::warn!("[paths] Command::SetPresetsPath failed: {e}");
    }
}

/// Same as [`apply_presets_path`] but for the plugins override.
fn apply_plugins_path(
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
        .dispatch(Command::SetPluginsPath { path })
    {
        log::warn!("[paths] Command::SetPluginsPath failed: {e}");
    }
}

/// #582: same persist+dispatch pattern for the evaluations directory.
fn apply_evaluations_path(
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
    if let Err(e) = session
        .dispatcher
        .dispatch(Command::SetEvaluationsPath { path })
    {
        log::warn!("[paths] Command::SetEvaluationsPath failed: {e}");
    }
}

/// #561: dispatch `Command::ReloadPluginCatalog` and return a
/// human-readable summary of the new totals (or an error message
/// suitable for the status text). Both `install` and `install_secondary`
/// share this helper so the success/failure path is one place.
///
/// When no project session is attached we still dispatch through a
/// fresh in-process dispatcher snapshot — the catalog is process-wide
/// state, not project-scoped. This mirrors the boot path
/// (`init_many`), which runs before any project is loaded.
fn run_reload_plugin_catalog(project_session: &Rc<RefCell<Option<ProjectSession>>>) -> String {
    // Use the session's dispatcher when available so other listeners
    // (publishing fan-out) see the event; fall back to a one-shot
    // local dispatcher when no project is loaded (still triggers the
    // registry reload because the handler reaches the same process-
    // wide `plugin_loader::registry`).
    let events_result: anyhow::Result<Vec<Event>> = {
        let borrow = project_session.borrow();
        if let Some(session) = borrow.as_ref() {
            session.dispatcher.dispatch(Command::ReloadPluginCatalog)
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
                .dispatch(Command::ReloadPluginCatalog)
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
    win.on_pick_presets_path(move || {
        let Some(path) = pick_folder_dialog() else {
            return;
        };
        apply_presets_path(&session, &config, Some(path.clone()));
        if let Some(w) = win_weak.upgrade() {
            w.set_presets_path(path.to_string_lossy().into_owned().into());
        }
    });

    // ── presets / Reset ─────────────────────────────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    win.on_reset_presets_path(move || {
        apply_presets_path(&session, &config, None);
        if let Some(w) = win_weak.upgrade() {
            w.set_presets_path(slint::SharedString::default());
        }
    });

    // ── plugins / Choose ────────────────────────────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    win.on_pick_plugins_path(move || {
        let Some(path) = pick_folder_dialog() else {
            return;
        };
        apply_plugins_path(&session, &config, Some(path.clone()));
        if let Some(w) = win_weak.upgrade() {
            w.set_plugins_path(path.to_string_lossy().into_owned().into());
        }
    });

    // ── plugins / Reset ─────────────────────────────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    win.on_reset_plugins_path(move || {
        apply_plugins_path(&session, &config, None);
        if let Some(w) = win_weak.upgrade() {
            w.set_plugins_path(slint::SharedString::default());
        }
    });

    // ── evaluations / Choose (#582) ─────────────────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    win.on_pick_evaluations_path(move || {
        let Some(path) = pick_folder_dialog() else {
            return;
        };
        apply_evaluations_path(&session, &config, Some(path.clone()));
        if let Some(w) = win_weak.upgrade() {
            w.set_evaluations_path(path.to_string_lossy().into_owned().into());
        }
    });

    // ── evaluations / Reset (#582) ──────────────────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    win.on_reset_evaluations_path(move || {
        apply_evaluations_path(&session, &config, None);
        if let Some(w) = win_weak.upgrade() {
            w.set_evaluations_path(slint::SharedString::default());
        }
    });

    // ── #561 reload plugin catalog ──────────────────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    win.on_reload_plugin_catalog(move || {
        let status = run_reload_plugin_catalog(&session);
        if let Some(w) = win_weak.upgrade() {
            w.set_plugin_catalog_status(status.into());
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
    win.on_pick_presets_path(move || {
        let Some(path) = pick_folder_dialog() else {
            return;
        };
        apply_presets_path(&session, &config, Some(path.clone()));
        if let Some(w) = win_weak.upgrade() {
            w.set_presets_path(path.to_string_lossy().into_owned().into());
        }
    });

    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    win.on_reset_presets_path(move || {
        apply_presets_path(&session, &config, None);
        if let Some(w) = win_weak.upgrade() {
            w.set_presets_path(slint::SharedString::default());
        }
    });

    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    win.on_pick_plugins_path(move || {
        let Some(path) = pick_folder_dialog() else {
            return;
        };
        apply_plugins_path(&session, &config, Some(path.clone()));
        if let Some(w) = win_weak.upgrade() {
            w.set_plugins_path(path.to_string_lossy().into_owned().into());
        }
    });

    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    win.on_reset_plugins_path(move || {
        apply_plugins_path(&session, &config, None);
        if let Some(w) = win_weak.upgrade() {
            w.set_plugins_path(slint::SharedString::default());
        }
    });

    // ── evaluations (#582) — secondary window ───────────────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    win.on_pick_evaluations_path(move || {
        let Some(path) = pick_folder_dialog() else {
            return;
        };
        apply_evaluations_path(&session, &config, Some(path.clone()));
        if let Some(w) = win_weak.upgrade() {
            w.set_evaluations_path(path.to_string_lossy().into_owned().into());
        }
    });

    let win_weak = win.as_weak();
    let session = project_session.clone();
    let config = app_config.clone();
    win.on_reset_evaluations_path(move || {
        apply_evaluations_path(&session, &config, None);
        if let Some(w) = win_weak.upgrade() {
            w.set_evaluations_path(slint::SharedString::default());
        }
    });

    // ── #561 reload plugin catalog (secondary window) ───────────────
    let win_weak = win.as_weak();
    let session = project_session.clone();
    win.on_reload_plugin_catalog(move || {
        let status = run_reload_plugin_catalog(&session);
        if let Some(w) = win_weak.upgrade() {
            w.set_plugin_catalog_status(status.into());
        }
    });
}

/// Seed the initial `presets-path` / `plugins-path` /
/// `evaluations-path` Slint properties from the persisted
/// `AppConfig.paths` snapshot so the Settings screen renders the
/// user's current choice on first open. Called once at startup from
/// `desktop_app::setup`.
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
    let evaluations = config
        .paths
        .evaluations_path
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    win.set_presets_path(presets.into());
    win.set_plugins_path(plugins.into());
    win.set_evaluations_path(evaluations.into());
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
    let evaluations = config
        .paths
        .evaluations_path
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    win.set_presets_path(presets.into());
    win.set_plugins_path(plugins.into());
    win.set_evaluations_path(evaluations.into());
}
