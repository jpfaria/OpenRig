//! Block-scoped commands: parameter writes, enable/model changes, block CRUD
//! inside a chain, and the Insert block's I/O binding.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use domain::ids::{BlockId, ChainId};
use project::block::AudioBlock;

/// Every state change scoped to a single block inside a chain.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum BlockCommand {
    // ── Block parameters ────────────────────────────────────────────────────
    /// Set a numeric (f64) parameter on a block.
    SetBlockParameterNumber {
        chain: ChainId,
        block: BlockId,
        path: String,
        value: f64,
    },

    /// Set a bool parameter on a block.
    SetBlockParameterBool {
        chain: ChainId,
        block: BlockId,
        path: String,
        value: bool,
    },

    /// Set a text (string) parameter on a block.
    SetBlockParameterText {
        chain: ChainId,
        block: BlockId,
        path: String,
        value: String,
    },

    /// Select an enum/option parameter.
    ///
    /// `value` is the canonical option string (already resolved from UI index
    /// by the adapter before dispatching). `index` is kept for round-trip
    /// convenience when the caller needs to re-render the selected row.
    SelectBlockParameterOption {
        chain: ChainId,
        block: BlockId,
        path: String,
        /// The option value string as declared in the model schema.
        value: String,
        /// The UI index of the selected option (informational; not stored in the project).
        index: usize,
    },

    /// Open a file dialog and use the result as a text parameter value.
    PickBlockParameterFile {
        chain: ChainId,
        block: BlockId,
        path: String,
        file: PathBuf,
    },

    // ── Block enable / model ─────────────────────────────────────────────────
    /// Toggle the enabled flag of a block.
    ToggleBlockEnabled { chain: ChainId, block: BlockId },

    /// Replace the model (effect type + model_id) of a block.
    ReplaceBlockModel {
        chain: ChainId,
        block: BlockId,
        model_id: String,
    },

    // ── Block CRUD ────────────────────────────────────────────────────────────
    /// Insert a new block at `position` in the chain.
    AddBlock {
        chain: ChainId,
        kind: String,
        model_id: String,
        position: usize,
    },

    /// Insert a fully-constructed `AudioBlock` at `position` in the chain.
    ///
    /// Unlike `AddBlock`, the caller is responsible for building the block
    /// (including its kind and parameters). The block's `id` is preserved
    /// as-is — the caller must supply a unique id within the chain.
    InsertPrebuiltBlock {
        chain: ChainId,
        block: AudioBlock,
        position: usize,
    },

    /// Overwrite the block with the given `block_id` in-place.
    ///
    /// The dispatcher locates the block by `block_id` and replaces it with
    /// the provided `replacement`. The replacement's `id` field is ignored —
    /// the original `block_id` is preserved on the stored block.
    OverwriteBlock {
        chain: ChainId,
        block: BlockId,
        replacement: AudioBlock,
    },

    /// Remove a block from a chain.
    RemoveBlock { chain: ChainId, block: BlockId },

    /// Move a block to `new_position` within its chain.
    MoveBlock {
        chain: ChainId,
        block: BlockId,
        new_position: usize,
    },

    // ── Insert block ──────────────────────────────────────────────────────────
    /// Commit an Insert block's I/O binding selection (#716, model A).
    ///
    /// The caller supplies the registry binding id (`io`) for the external
    /// send/return loop: the send goes to that binding's output, the return
    /// comes from its input — both resolved from the per-machine registry. The
    /// dispatcher locates the block and replaces its `InsertBlock.io`.
    SaveInsertBlock {
        chain: ChainId,
        block: BlockId,
        io: String,
    },
}
