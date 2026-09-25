//! Responsibility: writes a path override into the machine's config.

use infra_filesystem::{AppConfig, FilesystemStorage};
use std::path::PathBuf;

/// #607: Apply a **presets** path override — persist it into `config.yaml`
/// AND mirror it into the shared in-memory `AppConfig`, which the GUI reads.
/// The mirror was the #607 fix when project-open / register-recent wrote that
/// whole snapshot back to disk; since #980 they write only `recent_projects`,
/// and the mirror keeps what the GUI shows in step with the file.
pub fn apply_presets_override(config: &mut AppConfig, path: Option<PathBuf>) -> anyhow::Result<()> {
    apply_presets_override_at(&FilesystemStorage::app_config_path()?, config, path)
}

/// [`apply_presets_override`] against an explicit config file, so a test can
/// drive the persist + mirror pair without writing the machine's real
/// `config.yaml` (#701: a test that did clobbered the owner's setup).
pub fn apply_presets_override_at(
    config_path: &std::path::Path,
    config: &mut AppConfig,
    path: Option<PathBuf>,
) -> anyhow::Result<()> {
    FilesystemStorage::update_app_config_at(config_path, |c| c.paths.presets_path = path.clone())?;
    config.paths.presets_path = path;
    Ok(())
}

/// #607: same persist + in-memory mirror for the **plugins** override.
pub fn apply_plugins_override(config: &mut AppConfig, path: Option<PathBuf>) -> anyhow::Result<()> {
    apply_plugins_override_at(&FilesystemStorage::app_config_path()?, config, path)
}

/// [`apply_plugins_override`] against an explicit config file.
pub fn apply_plugins_override_at(
    config_path: &std::path::Path,
    config: &mut AppConfig,
    path: Option<PathBuf>,
) -> anyhow::Result<()> {
    FilesystemStorage::update_app_config_at(config_path, |c| c.paths.plugins_path = path.clone())?;
    config.paths.plugins_path = path;
    Ok(())
}

/// #607: same persist + in-memory mirror for the **evaluations** override.
pub fn apply_evaluations_override(
    config: &mut AppConfig,
    path: Option<PathBuf>,
) -> anyhow::Result<()> {
    apply_evaluations_override_at(&FilesystemStorage::app_config_path()?, config, path)
}

/// [`apply_evaluations_override`] against an explicit config file.
pub fn apply_evaluations_override_at(
    config_path: &std::path::Path,
    config: &mut AppConfig,
    path: Option<PathBuf>,
) -> anyhow::Result<()> {
    FilesystemStorage::update_app_config_at(config_path, |c| {
        c.paths.evaluations_path = path.clone()
    })?;
    config.paths.evaluations_path = path;
    Ok(())
}
