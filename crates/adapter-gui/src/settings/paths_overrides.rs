//! Responsibility: writes a path override into the machine's config.

use infra_filesystem::{AppConfig, FilesystemStorage};
use std::path::PathBuf;

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
