//! I/O binding registry command handlers (#716).
//!
//! Create / Update (upsert by id) / Delete operations on the per-machine
//! `config.yaml` I/O binding registry. Every handler reads the current
//! `AppConfig`, applies the mutation, and queues a persist-worker write so
//! the caller (GUI/MCP thread) never waits on disk I/O.
//!
//! Reference-checking for Delete (reject when a chain block references the
//! id) is deferred to Task 5. The single insertion point is marked with
//! `TODO(#716-task5)` so it can be added once chain blocks carry binding ids.

use anyhow::Result;

use domain::io_binding::IoBinding;
use infra_filesystem::FilesystemStorage;

use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

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
        crate::persist_worker::run(move || {
            let mut config = match FilesystemStorage::load_app_config() {
                Ok(c) => c,
                Err(e) => {
                    let path = FilesystemStorage::app_config_path()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| "<unresolvable>".to_string());
                    log::error!(
                        "io_binding create/update: failed to load config from {path}: {e} — \
                         proceeding with default (existing data may be lost)"
                    );
                    Default::default()
                }
            };
            if let Some(pos) = config.io_bindings.iter().position(|b| b.id == binding.id) {
                config.io_bindings[pos] = binding;
            } else {
                config.io_bindings.push(binding);
            }
            if let Err(e) = FilesystemStorage::save_app_config(&config) {
                log::error!("failed to persist io_bindings after create/update: {e}");
            }
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
        crate::persist_worker::run(move || {
            let mut config = match FilesystemStorage::load_app_config() {
                Ok(c) => c,
                Err(e) => {
                    let path = FilesystemStorage::app_config_path()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| "<unresolvable>".to_string());
                    log::error!(
                        "io_binding delete: failed to load config from {path}: {e} — \
                         proceeding with default (existing data may be lost)"
                    );
                    Default::default()
                }
            };
            config.io_bindings.retain(|b| b.id != id);
            if let Err(e) = FilesystemStorage::save_app_config(&config) {
                log::error!("failed to persist io_bindings after delete: {e}");
            }
        });
        Ok(vec![Event::IoBindingRegistryChanged])
    }
}
