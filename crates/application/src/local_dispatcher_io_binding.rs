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
    /// Apply one registry mutation to BOTH copies of the effective registry:
    /// the in-memory one the frontend renders from and re-installs into the
    /// controller on every sync (#127), and the persisted per-machine
    /// `config.yaml`, written off-thread.
    ///
    /// Doing only the second is what made an edit issued off the GUI evaporate:
    /// the next chain sync re-installed the untouched in-memory registry over
    /// it. `mutate` therefore runs twice — once per copy — and must be `Fn`.
    fn update_registry(
        &self,
        what: &'static str,
        mutate: impl Fn(&mut Vec<IoBinding>) + Send + 'static,
    ) {
        if let Some(registry) = self.io_bindings.borrow().as_ref() {
            mutate(&mut registry.borrow_mut());
        }
        // Resolve the path on the dispatching thread (no Send requirement on
        // the RefCell borrow), then move it into the closure.
        let config_path = resolve_config_path(self.io_config_path.borrow().clone());
        crate::persist_worker::run(move || {
            let Some(path) = config_path else {
                log::error!("io_binding {what}: config path unresolvable — not persisted");
                return;
            };
            if let Err(e) = FilesystemStorage::update_app_config_at(&path, |config| {
                mutate(&mut config.io_bindings)
            }) {
                log::error!("io_binding {what}: persist failed: {e}");
            }
        });
    }

    /// Handle `Command::CreateIoBinding` and `Command::UpdateIoBinding`.
    ///
    /// Both operations are upserts keyed on `binding.id`: if an entry with
    /// the same id exists it is replaced; otherwise the binding is appended.
    /// Persistence is queued on the async persist worker.
    pub(crate) fn handle_create_or_update_io_binding(
        &self,
        binding: IoBinding,
    ) -> Result<Vec<Event>> {
        self.update_registry("create/update", move |list| {
            match list.iter().position(|b| b.id == binding.id) {
                Some(pos) => list[pos] = binding.clone(),
                None => list.push(binding.clone()),
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

        self.update_registry("delete", move |list| list.retain(|b| b.id != id));
        Ok(vec![Event::IoBindingRegistryChanged])
    }

    /// Handle `Command::RenameIoBinding`: rename the entry whose `id` matches
    /// and persist. No-op when the id is absent.
    pub(crate) fn handle_rename_io_binding(&self, id: String, name: String) -> Result<Vec<Event>> {
        self.update_registry("rename", move |list| {
            if let Some(b) = list.iter_mut().find(|b| b.id == id) {
                b.name.clone_from(&name);
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
        // The name is sequential ("In N"), so deriving it from each copy's own
        // length would hand the two copies DIFFERENT names the moment they
        // disagree. Derive it once, from the registry the dispatcher owns, and
        // apply that one value to both.
        let fixed_name = self.next_endpoint_name_for(&binding_id, is_input);
        self.update_registry("add endpoint", move |list| {
            let Some(b) = list.iter_mut().find(|b| b.id == binding_id) else {
                return;
            };
            let side = if is_input {
                &mut b.inputs
            } else {
                &mut b.outputs
            };
            let endpoint = IoEndpoint {
                name: fixed_name
                    .clone()
                    .unwrap_or_else(|| next_endpoint_name(side.len(), is_input)),
                device_id: DeviceId(device_id.clone()),
                mode,
                channels: channels.clone(),
            };
            side.push(endpoint);
        });
        Ok(vec![Event::IoBindingRegistryChanged])
    }

    /// The auto-assigned endpoint name for an `AddIoEndpoint`, derived ONCE
    /// from the registry the dispatcher owns so both copies get the same value.
    ///
    /// `None` when no registry is attached (or it does not carry this binding):
    /// then only the persisted copy is written, so deriving the name from that
    /// copy's own length cannot diverge from anything.
    fn next_endpoint_name_for(&self, binding_id: &str, is_input: bool) -> Option<String> {
        let attached = self.io_bindings.borrow();
        let bindings = attached.as_ref()?.borrow();
        let binding = bindings.iter().find(|b| b.id == binding_id)?;
        let existing = if is_input {
            binding.inputs.len()
        } else {
            binding.outputs.len()
        };
        Some(next_endpoint_name(existing, is_input))
    }

    /// Handle `Command::RemoveIoEndpoint`: drop the named endpoint from the
    /// matching side and persist.
    pub(crate) fn handle_remove_io_endpoint(
        &self,
        binding_id: String,
        is_input: bool,
        endpoint_name: String,
    ) -> Result<Vec<Event>> {
        self.update_registry("remove endpoint", move |list| {
            if let Some(b) = list.iter_mut().find(|b| b.id == binding_id) {
                if is_input {
                    b.inputs.retain(|e| e.name != endpoint_name);
                } else {
                    b.outputs.retain(|e| e.name != endpoint_name);
                }
            }
        });
        Ok(vec![Event::IoBindingRegistryChanged])
    }

    /// Handle `Command::SetIoBindings` (#127, AUDIO-CRITICAL): install the
    /// effective registry into the live audio runtime so an ALREADY RUNNING
    /// rig re-resolves its device endpoints against the latest edit. Nothing
    /// is persisted here — the CRUD handlers above own `config.yaml`.
    ///
    /// The registry installed is the dispatcher's own (attached by the
    /// frontend, mutated by those CRUD handlers), never a caller-supplied
    /// list: the frontend re-installs the same handle on every chain sync, so
    /// anything else would be reverted moments later.
    pub(crate) fn handle_set_io_bindings(&self) -> Result<Vec<Event>> {
        // Clone out first: nothing may stay borrowed across the call into the
        // frontend's runtime.
        let bindings = self
            .io_bindings
            .borrow()
            .as_ref()
            .map(|registry| registry.borrow().clone());
        if let Some(bindings) = bindings {
            if let Some(control) = self.runtime_control() {
                control.set_io_bindings(bindings);
            }
        }
        Ok(vec![Event::IoBindingRegistryChanged])
    }
}

/// Sequential endpoint name ("In N" / "Out N") so an added endpoint is always
/// labelled without the GUI inventing a name.
fn next_endpoint_name(existing: usize, is_input: bool) -> String {
    let prefix = if is_input { "In" } else { "Out" };
    format!("{prefix} {}", existing + 1)
}
