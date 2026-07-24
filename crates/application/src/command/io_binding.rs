//! I/O binding registry commands (#716): CRUD over the per-machine bindings
//! stored in `config.yaml`, plus their endpoints.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use domain::io_binding::{ChannelMode, IoBinding};

/// Every state change scoped to the per-machine I/O binding registry.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum IoBindingCommand {
    // ── I/O binding registry (#716) ───────────────────────────────────────────
    /// #716: add a new I/O binding to the per-machine registry in
    /// `config.yaml`. The binding is identified by `binding.id`.
    /// When an entry with the same `id` already exists it is replaced
    /// (upsert semantics) so callers may treat create and update as one
    /// operation. Persists via the async persist worker (no blocking).
    CreateIoBinding { binding: IoBinding },

    /// #716: update an existing I/O binding in the per-machine registry.
    /// Locates the entry whose `id` matches `binding.id` and replaces it
    /// in-place; if no entry with that `id` exists the binding is appended
    /// (same upsert semantics as `CreateIoBinding`). Persists via the
    /// async persist worker.
    UpdateIoBinding { binding: IoBinding },

    /// #716: remove an I/O binding from the per-machine registry.
    ///
    /// Note: reference-checking (reject when a chain block references `id`)
    /// is deferred to Task 5. The handler below has a clear single point
    /// marked `TODO(#716-task5)` where the guard can be inserted when chain
    /// blocks reference bindings.
    DeleteIoBinding { id: String },

    /// #716: rename an existing I/O binding. The handler renames the entry
    /// whose `id` matches and persists; the GUI only forwards id + new name.
    RenameIoBinding { id: String, name: String },

    /// #716: add an endpoint to an I/O binding. The handler builds the
    /// `IoEndpoint` (auto-assigned "In N" / "Out N" name), appends it to the
    /// binding's inputs (or outputs) and persists. The GUI forwards only the
    /// structured picker values — it does NOT construct the domain endpoint.
    AddIoEndpoint {
        binding_id: String,
        is_input: bool,
        device_id: String,
        channels: Vec<usize>,
        mode: ChannelMode,
    },

    /// #716: remove the named endpoint from a binding's inputs (or outputs)
    /// and persist. The GUI forwards only the identifiers.
    RemoveIoEndpoint {
        binding_id: String,
        is_input: bool,
        endpoint_name: String,
    },
}
