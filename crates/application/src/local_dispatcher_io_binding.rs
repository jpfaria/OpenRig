//! I/O binding registry command handlers (#716).
//!
//! Create / Update (upsert by id) / Delete operations on the per-machine
//! `config.yaml` I/O binding registry. Every handler reads the current
//! `AppConfig`, applies the mutation, and queues a persist-worker write so
//! the caller (GUI/MCP thread) never waits on disk I/O.
//!
//! ## Path resolution
//!
//! Every handler reads `self.config_path` (set via `attach_config_path`)
//! to determine where to persist. When no path is attached, falls back to
//! `FilesystemStorage::app_config_path()` — the same resolution the global
//! `load_app_config` / `save_app_config` helpers use. This mirrors the
//! pattern in `local_dispatcher_project.rs` `save_project_to_disk` (lines
//! that read `self.config_path.borrow().clone().unwrap_or_else(…)`).
//!
//! Tests attach a temp-dir path via `attach_config_path` so no global OS
//! path (e.g. `~/Library/Application Support/OpenRig/config.yaml`) is ever
//! touched.
//!
//! Reference-checking for Delete (reject when a chain block references the
//! id) is deferred to Task 5. The single insertion point is marked with
//! `TODO(#716-task5)` so it can be added once chain blocks carry binding ids.

use std::path::PathBuf;

use anyhow::Result;

use domain::io_binding::IoBinding;
use infra_filesystem::{AppConfig, FilesystemStorage};

use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

// ── Path resolution ──────────────────────────────────────────────────────────

/// Resolve the effective config path: attached override first, OS default
/// fallback. Returns `None` only when `FilesystemStorage::app_config_path()`
/// itself fails (unresolvable HOME / XDG).
fn resolve_config_path(attached: Option<PathBuf>) -> Option<PathBuf> {
    attached.or_else(|| FilesystemStorage::app_config_path().ok())
}

/// Load `AppConfig` from `path`. Returns `Default::default()` on any error,
/// logging it so a corrupt config is never silently wiped.
fn load_config_at(path: &PathBuf) -> AppConfig {
    if !path.exists() {
        return AppConfig::default();
    }
    match std::fs::read_to_string(path) {
        Ok(raw) => match serde_yaml::from_str::<AppConfig>(&raw) {
            Ok(cfg) => cfg,
            Err(e) => {
                log::error!(
                    "io_binding: failed to parse config at {}: {e} — \
                     proceeding with default (existing data may be lost)",
                    path.display()
                );
                AppConfig::default()
            }
        },
        Err(e) => {
            log::error!(
                "io_binding: failed to read config at {}: {e} — \
                 proceeding with default (existing data may be lost)",
                path.display()
            );
            AppConfig::default()
        }
    }
}

/// Persist `config` to `path`, creating parent directories as needed.
fn save_config_at(path: &PathBuf, config: &AppConfig) {
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::error!(
                "io_binding: failed to create config dir {}: {e}",
                parent.display()
            );
            return;
        }
    }
    match serde_yaml::to_string(config) {
        Ok(raw) => {
            if let Err(e) = std::fs::write(path, raw) {
                log::error!(
                    "io_binding: failed to write config to {}: {e}",
                    path.display()
                );
            }
        }
        Err(e) => {
            log::error!("io_binding: failed to serialize AppConfig: {e}");
        }
    }
}

// ── Handlers ─────────────────────────────────────────────────────────────────

impl LocalDispatcher {
    /// Handle `Command::CreateIoBinding` and `Command::UpdateIoBinding`.
    ///
    /// Both operations are upserts keyed on `binding.id`: if an entry with
    /// the same id exists it is replaced; otherwise the binding is appended.
    /// Persistence is queued on the async persist worker.
    pub(crate) fn handle_create_or_update_io_binding(
        &self,
        binding: IoBinding,
    ) -> Result<Vec<Event>> {
        // Resolve the path on the dispatching thread (no Send requirement on
        // the RefCell borrow), then move it into the closure.
        let config_path = resolve_config_path(self.config_path.borrow().clone());
        crate::persist_worker::run(move || {
            let Some(path) = config_path else {
                log::error!(
                    "io_binding create/update: config path unresolvable — \
                     binding not persisted"
                );
                return;
            };
            let mut config = load_config_at(&path);
            if let Some(pos) = config.io_bindings.iter().position(|b| b.id == binding.id) {
                config.io_bindings[pos] = binding;
            } else {
                config.io_bindings.push(binding);
            }
            save_config_at(&path, &config);
        });
        Ok(vec![Event::IoBindingRegistryChanged])
    }

    /// Handle `Command::DeleteIoBinding`.
    ///
    /// Removes the binding with `id` from `config.yaml`. No-op when the id
    /// is not present (idempotent).
    ///
    /// TODO(#716-task5): add a reference-check here — scan
    /// `self.project.borrow().chains` for any block that references `id`
    /// and return `Err` when found, naming the referencing chain.
    pub(crate) fn handle_delete_io_binding(&self, id: String) -> Result<Vec<Event>> {
        let config_path = resolve_config_path(self.config_path.borrow().clone());
        crate::persist_worker::run(move || {
            let Some(path) = config_path else {
                log::error!(
                    "io_binding delete: config path unresolvable — \
                     binding not removed from disk"
                );
                return;
            };
            let mut config = load_config_at(&path);
            config.io_bindings.retain(|b| b.id != id);
            save_config_at(&path, &config);
        });
        Ok(vec![Event::IoBindingRegistryChanged])
    }
}
