//! Responsibility: seeds the paths section with what is already configured.

use crate::{AppWindow, ProjectSettingsWindow};
use infra_filesystem::FilesystemStorage;
use slint::Global;

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
    crate::SettingsBridge::get(win).set_presets_path(presets.into());
    crate::SettingsBridge::get(win).set_plugins_path(plugins.into());
    crate::SettingsBridge::get(win).set_evaluations_path(evaluations.into());
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
    crate::SettingsBridge::get(win).set_presets_path(presets.into());
    crate::SettingsBridge::get(win).set_plugins_path(plugins.into());
    crate::SettingsBridge::get(win).set_evaluations_path(evaluations.into());
}
