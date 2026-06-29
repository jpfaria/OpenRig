//! Read/modify/write of the per-machine `config.yaml` (`AppConfig`).
//!
//! Split out of `lib.rs` (size cap) in #731. The `*_at` variants take an
//! EXPLICIT config path so a caller can BIND the destination at dispatch
//! time and hand it to the async persist worker — the worker must never
//! re-resolve `$HOME` at write time. When a HOME-swap test restores the
//! real `$HOME` before the queued write lands, a write that re-resolves
//! `$HOME` leaks the fixtures onto the user's real config (#701/#731).
//! The no-argument wrappers resolve `app_config_path()` once and delegate.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::{AppConfig, FilesystemStorage, GuiSystemSettings, LegacyGuiAudioSettings};

impl FilesystemStorage {
    /// Read GUI audio settings (input/output devices + language) from
    /// the unified `config.yaml`. Issue #287: previously these lived in
    /// a separate `gui-settings.yaml`, now folded into `AppConfig`.
    pub fn load_gui_audio_settings() -> Result<Option<GuiSystemSettings>> {
        let config = Self::load_app_config()?;
        if config.input_devices.is_empty()
            && config.output_devices.is_empty()
            && config.language.is_none()
            && config.midi_devices.is_empty()
        {
            return Ok(None);
        }
        Ok(Some(GuiSystemSettings {
            input_devices: config.input_devices,
            output_devices: config.output_devices,
            language: config.language,
            midi_devices: config.midi_devices,
        }))
    }

    /// Persist GUI audio settings into `config.yaml`, preserving the
    /// other AppConfig fields (recent_projects, paths).
    pub fn save_gui_audio_settings(settings: &GuiSystemSettings) -> Result<()> {
        Self::update_app_config(|config| {
            config.input_devices = settings.input_devices.clone();
            config.output_devices = settings.output_devices.clone();
            config.language = settings.language.clone();
            config.midi_devices = settings.midi_devices.clone();
        })
    }

    /// Update only the `language` field, preserving every other config
    /// field. Used by the language selector so picking a new locale
    /// doesn't clobber audio device selection.
    pub fn save_gui_language(language: Option<String>) -> Result<()> {
        Self::update_app_config(|config| config.language = language)
    }

    /// #513: update only the user's preset directory override (under
    /// `AppConfig.paths.presets_path`), preserving every other config
    /// field. `None` resets the override so the OS default wins again.
    pub fn save_presets_path(path: Option<PathBuf>) -> Result<()> {
        Self::update_app_config(|config| config.paths.presets_path = path)
    }

    /// #513: update only the user's plugin directory override (under
    /// `AppConfig.paths.plugins_path`), preserving every other config
    /// field. `None` resets the override so the OS default wins again.
    pub fn save_plugins_path(path: Option<PathBuf>) -> Result<()> {
        Self::update_app_config(|config| config.paths.plugins_path = path)
    }

    /// #582: update only the user's evaluations directory override
    /// (under `AppConfig.paths.evaluations_path`), preserving every other
    /// config field. `None` resets the override so the OS default
    /// ([`default_evaluations_path`]) wins again.
    pub fn save_evaluations_path(path: Option<PathBuf>) -> Result<()> {
        Self::update_app_config(|config| config.paths.evaluations_path = path)
    }

    pub fn load_app_config() -> Result<AppConfig> {
        Self::load_app_config_at(&Self::app_config_path()?)
    }

    /// Read `config.yaml` from an EXPLICIT path (#731: caller binds the
    /// path at dispatch time). The legacy `gui-settings.yaml` migration
    /// reads from the same directory as `config_path`.
    pub fn load_app_config_at(config_path: &Path) -> Result<AppConfig> {
        log::info!("loading app config from {:?}", config_path);
        let mut config = if config_path.exists() {
            let raw = fs::read_to_string(config_path)
                .with_context(|| format!("failed to read app config from {:?}", config_path))?;
            serde_yaml::from_str::<AppConfig>(&raw)
                .with_context(|| format!("failed to parse app config from {:?}", config_path))?
        } else {
            log::debug!("app config file not found, using defaults");
            AppConfig::default()
        };
        // Issue #287: migrate the historical `gui-settings.yaml` into
        // `config.yaml` on first load, then delete the legacy file. Any
        // fields already in config.yaml win — old gui-settings only
        // fills empty slots. The legacy file sits beside `config.yaml`.
        let legacy_path = config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("gui-settings.yaml");
        Self::migrate_gui_settings_into(&mut config, &legacy_path)?;
        Ok(config)
    }

    /// Best-effort migration: read `gui-settings.yaml` if it still
    /// exists, fold its fields into the current AppConfig (only when
    /// the AppConfig slot is empty), persist, then remove the legacy
    /// file. Failures log but do not propagate so a corrupted legacy
    /// file can't block boot.
    fn migrate_gui_settings_into(config: &mut AppConfig, legacy_path: &Path) -> Result<()> {
        if !legacy_path.exists() {
            return Ok(());
        }
        let raw = match fs::read_to_string(legacy_path) {
            Ok(content) => content,
            Err(error) => {
                log::warn!(
                    "could not read legacy gui-settings.yaml at {:?}: {error}",
                    legacy_path
                );
                return Ok(());
            }
        };
        let legacy: GuiSystemSettings = match serde_yaml::from_str::<GuiSystemSettings>(&raw) {
            Ok(value) => value,
            Err(_) => match serde_yaml::from_str::<LegacyGuiAudioSettings>(&raw) {
                Ok(legacy) => legacy.into(),
                Err(error) => {
                    log::warn!(
                        "legacy gui-settings.yaml at {:?} unreadable, leaving in place: {error}",
                        legacy_path
                    );
                    return Ok(());
                }
            },
        };
        if config.input_devices.is_empty() {
            config.input_devices = legacy.input_devices;
        }
        if config.output_devices.is_empty() {
            config.output_devices = legacy.output_devices;
        }
        if config.language.is_none() {
            config.language = legacy.language;
        }
        // Persist the merged result before deleting the source so a
        // crash mid-migration doesn't lose data. The merged config sits
        // beside the legacy file.
        let config_path = legacy_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("config.yaml");
        Self::save_app_config_at(&config_path, config)?;
        if let Err(error) = fs::remove_file(legacy_path) {
            log::warn!(
                "merged gui-settings.yaml into config.yaml but couldn't remove legacy file at {:?}: {error}",
                legacy_path
            );
        } else {
            log::info!("migrated gui-settings.yaml into config.yaml; removed legacy file");
        }
        Ok(())
    }

    pub fn save_app_config(config: &AppConfig) -> Result<()> {
        Self::save_app_config_at(&Self::app_config_path()?, config)
    }

    /// Write `config.yaml` to an EXPLICIT path (#731: caller binds the
    /// path at dispatch time so the async persist worker never
    /// re-resolves `$HOME`).
    pub fn save_app_config_at(config_path: &Path, config: &AppConfig) -> Result<()> {
        log::info!("saving app config to {:?}", config_path);
        let parent = config_path
            .parent()
            .context("app config path has no parent directory")?;
        log::debug!("ensuring directory exists: {:?}", parent);
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create app config directory {:?}", parent))?;
        let raw = serde_yaml::to_string(config)?;
        fs::write(config_path, raw)
            .with_context(|| format!("failed to write app config to {:?}", config_path))?;
        Ok(())
    }

    /// Read-modify-write `config.yaml` at the resolved path. Convenience
    /// for the no-argument save_* wrappers; the persist worker uses
    /// [`Self::update_app_config_at`] with a dispatch-bound path instead.
    fn update_app_config(mutate: impl FnOnce(&mut AppConfig)) -> Result<()> {
        Self::update_app_config_at(&Self::app_config_path()?, mutate)
    }

    /// Read-modify-write `config.yaml` at an EXPLICIT path (#731). Loads
    /// the current config (or default), applies `mutate`, writes it back
    /// — all against `config_path`, never re-resolving `$HOME`.
    pub fn update_app_config_at(
        config_path: &Path,
        mutate: impl FnOnce(&mut AppConfig),
    ) -> Result<()> {
        let mut config = Self::load_app_config_at(config_path).unwrap_or_default();
        mutate(&mut config);
        Self::save_app_config_at(config_path, &config)
    }
}
