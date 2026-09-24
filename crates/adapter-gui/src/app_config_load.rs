//! Responsibility: loads the machine's config for the session that is starting.

use crate::project_ops_recents::sync_recent_projects;
use crate::state::AppConfigYaml;
use anyhow::Result;
use infra_filesystem::{AppConfig, FilesystemStorage};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn load_and_sync_app_config() -> Result<AppConfig> {
    let mut config = FilesystemStorage::load_app_config().unwrap_or_default();
    let changed = sync_recent_projects(&mut config);
    if changed {
        // #693: boot-time migration write goes to the persist worker.
        // #731: bind the config path at dispatch time.
        application::app_config_persist::persist_app_config_snapshot(config.clone());
    }
    Ok(config)
}

/// Where chain presets are saved when the project names no `presets_path`:
/// the user's data folder (`~/Library/Application Support/OpenRig/presets`,
/// `%APPDATA%\OpenRig\presets`, `~/.local/share/openrig/presets`), the same
/// default `openrig://paths` reports. The install folder is read-only on an
/// installed app (#978); a project can still point `presets_path` elsewhere in
/// its own `config.yaml`.
pub(crate) fn default_presets_path() -> PathBuf {
    infra_filesystem::user_data_root().join("presets")
}

/// The preset library that ships with the app, read-only, under the data
/// root (`<bundle>/Contents/Resources/presets` on macOS, `/usr/share/openrig`
/// on Linux, the install folder on Windows, the working directory in dev).
/// The picker lists it next to the user's presets.
pub(crate) fn bundled_presets_path() -> PathBuf {
    infra_filesystem::detect_data_root().join("presets")
}

pub(crate) fn load_app_config(path: &Path) -> Result<AppConfigYaml> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&raw)?)
}

pub(crate) fn resolve_project_config_path(project_path: &Path) -> PathBuf {
    project_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("config.yaml")
}

/// #716 Task 20: ensure the `"default"` I/O binding exists in the AppConfig at
/// `config_path`. If the binding is already present this is a no-op (idempotent).
/// If the config carries at least one input and one output device, a binding is
/// built from the first of each and persisted synchronously (new-project creation
/// is not on the audio thread, so a direct write is fine here).
pub(crate) fn ensure_default_io_binding(config_path: &Path) {
    use crate::default_io_binding::{build_default_io_binding, DEFAULT_BINDING_ID};

    // Load the full AppConfig from the given path (not the OS global path).
    let raw = match fs::read_to_string(config_path) {
        Ok(r) => r,
        Err(_) => return, // Config does not exist yet — no devices to bind.
    };
    let mut app_config: AppConfig = match serde_yaml::from_str(&raw) {
        Ok(c) => c,
        Err(_) => return, // Malformed config — leave it alone.
    };

    // Idempotent: do not add a second "default" binding.
    if app_config
        .io_bindings
        .iter()
        .any(|b| b.id == DEFAULT_BINDING_ID)
    {
        return;
    }

    let input_id = match app_config.input_devices.first() {
        Some(d) => d.device_id.clone(),
        None => return, // No input device configured — cannot build binding.
    };
    let output_id = match app_config.output_devices.first() {
        Some(d) => d.device_id.clone(),
        None => return, // No output device configured — cannot build binding.
    };

    let binding = build_default_io_binding(&input_id, &output_id);
    app_config.io_bindings.push(binding);

    if let Ok(serialized) = serde_yaml::to_string(&app_config) {
        let _ = fs::write(config_path, serialized);
    }
}

#[cfg(test)]
#[path = "app_config_load_tests.rs"]
mod tests;
