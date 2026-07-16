//! I/O binding registry command handlers (#716).
//!
//! Create / Update (upsert by id) / Delete operations on the per-machine
//! `config.yaml` I/O binding registry. Every handler reads the current
//! `AppConfig`, applies the mutation, and queues a persist-worker write so
//! the caller (GUI/MCP thread) never waits on disk I/O.
//!
//! ## Path resolution
//!
//! Every handler reads `self.io_config_path` (set via `attach_io_config_path`)
//! — the per-machine SYSTEM config, NOT the project sidecar `config_path`
//! (#792/ADR-0003: opening a project must not redirect the registry into the
//! project's `config.yaml`). When no path is attached, falls back to
//! `FilesystemStorage::app_config_path()` — the same resolution the global
//! `load_app_config` / `save_app_config` helpers use.
//!
//! Tests attach a temp-dir path via `attach_io_config_path` so no global OS
//! path (e.g. `~/Library/Application Support/OpenRig/config.yaml`) is ever
//! touched.
//!
//! Reference-checking for Delete (reject when a chain block references the
//! id) is deferred to Task 5. The single insertion point is marked with
//! `TODO(#716-task5)` so it can be added once chain blocks carry binding ids.

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use domain::ids::DeviceId;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use infra_filesystem::FilesystemStorage;
use project::block::AudioBlockKind;

use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

// ── Path resolution ──────────────────────────────────────────────────────────

/// Resolve the effective config path: attached override first, OS default
/// fallback. Returns `None` only when `FilesystemStorage::app_config_path()`
/// itself fails (unresolvable HOME / XDG).
fn resolve_config_path(attached: Option<PathBuf>) -> Option<PathBuf> {
    attached.or_else(|| FilesystemStorage::app_config_path().ok())
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
        let config_path = resolve_config_path(self.io_config_path.borrow().clone());
        crate::persist_worker::run(move || {
            let Some(path) = config_path else {
                log::error!(
                    "io_binding create/update: config path unresolvable — \
                     binding not persisted"
                );
                return;
            };
            if let Err(e) = FilesystemStorage::update_app_config_at(&path, |config| {
                if let Some(pos) = config.io_bindings.iter().position(|b| b.id == binding.id) {
                    config.io_bindings[pos] = binding;
                } else {
                    config.io_bindings.push(binding);
                }
            }) {
                log::error!("io_binding create/update: persist failed: {e}");
            }
        });
        Ok(vec![Event::IoBindingRegistryChanged])
    }

    /// Handle `Command::DeleteIoBinding`.
    ///
    /// Removes the binding with `id` from `config.yaml`. No-op when the id
    /// is not present (idempotent).
    ///
    /// Returns `Err` when any chain block in the current project references
    /// the binding via `block.io == id`, naming the first referencing chain.
    pub(crate) fn handle_delete_io_binding(&self, id: String) -> Result<Vec<Event>> {
        // O3: reject delete when any chain block references this binding id.
        let referencing_chain = self.project.borrow().chains.iter().find_map(|chain| {
            let referenced = chain.blocks.iter().any(|block| match &block.kind {
                AudioBlockKind::Input(ib) => ib.io == id,
                AudioBlockKind::Output(ob) => ob.io == id,
                _ => false,
            });
            if referenced {
                Some(chain.id.0.clone())
            } else {
                None
            }
        });
        if let Some(chain_id) = referencing_chain {
            return Err(anyhow!(
                "cannot delete binding '{}': referenced by chain '{}'",
                id,
                chain_id
            ));
        }

        let config_path = resolve_config_path(self.io_config_path.borrow().clone());
        crate::persist_worker::run(move || {
            let Some(path) = config_path else {
                log::error!(
                    "io_binding delete: config path unresolvable — \
                     binding not removed from disk"
                );
                return;
            };
            if let Err(e) = FilesystemStorage::update_app_config_at(&path, |config| {
                config.io_bindings.retain(|b| b.id != id);
            }) {
                log::error!("io_binding delete: persist failed: {e}");
            }
        });
        Ok(vec![Event::IoBindingRegistryChanged])
    }

    /// Handle `Command::RenameIoBinding`: rename the entry whose `id` matches
    /// and persist. No-op when the id is absent.
    pub(crate) fn handle_rename_io_binding(&self, id: String, name: String) -> Result<Vec<Event>> {
        let config_path = resolve_config_path(self.io_config_path.borrow().clone());
        crate::persist_worker::run(move || {
            let Some(path) = config_path else {
                log::error!("io_binding rename: config path unresolvable");
                return;
            };
            if let Err(e) = FilesystemStorage::update_app_config_at(&path, |config| {
                if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id) {
                    b.name = name;
                }
            }) {
                log::error!("io_binding rename: persist failed: {e}");
            }
        });
        Ok(vec![Event::IoBindingRegistryChanged])
    }

    /// Handle `Command::AddIoEndpoint`: build the `IoEndpoint` (auto-assigned
    /// "In N" / "Out N" name), append it to the binding's inputs (or outputs)
    /// and persist. The GUI never constructs the domain endpoint.
    pub(crate) fn handle_add_io_endpoint(
        &self,
        binding_id: String,
        is_input: bool,
        device_id: String,
        channels: Vec<usize>,
        mode: ChannelMode,
    ) -> Result<Vec<Event>> {
        let config_path = resolve_config_path(self.io_config_path.borrow().clone());
        crate::persist_worker::run(move || {
            let Some(path) = config_path else {
                log::error!("io_binding add endpoint: config path unresolvable");
                return;
            };
            if let Err(e) = FilesystemStorage::update_app_config_at(&path, |config| {
                if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == binding_id) {
                    let existing = if is_input {
                        b.inputs.len()
                    } else {
                        b.outputs.len()
                    };
                    let endpoint = IoEndpoint {
                        name: next_endpoint_name(existing, is_input),
                        device_id: DeviceId(device_id),
                        mode,
                        channels,
                    };
                    if is_input {
                        b.inputs.push(endpoint);
                    } else {
                        b.outputs.push(endpoint);
                    }
                }
            }) {
                log::error!("io_binding add endpoint: persist failed: {e}");
            }
        });
        Ok(vec![Event::IoBindingRegistryChanged])
    }

    /// Handle `Command::RemoveIoEndpoint`: drop the named endpoint from the
    /// matching side and persist.
    pub(crate) fn handle_remove_io_endpoint(
        &self,
        binding_id: String,
        is_input: bool,
        endpoint_name: String,
    ) -> Result<Vec<Event>> {
        let config_path = resolve_config_path(self.io_config_path.borrow().clone());
        crate::persist_worker::run(move || {
            let Some(path) = config_path else {
                log::error!("io_binding remove endpoint: config path unresolvable");
                return;
            };
            if let Err(e) = FilesystemStorage::update_app_config_at(&path, |config| {
                if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == binding_id) {
                    if is_input {
                        b.inputs.retain(|e| e.name != endpoint_name);
                    } else {
                        b.outputs.retain(|e| e.name != endpoint_name);
                    }
                }
            }) {
                log::error!("io_binding remove endpoint: persist failed: {e}");
            }
        });
        Ok(vec![Event::IoBindingRegistryChanged])
    }
}

/// Sequential endpoint name ("In N" / "Out N") so an added endpoint is always
/// labelled without the GUI inventing a name.
fn next_endpoint_name(existing: usize, is_input: bool) -> String {
    let prefix = if is_input { "In" } else { "Out" };
    format!("{prefix} {}", existing + 1)
}
