//! Typed `Command` enum — every state-change that any controller can request.
//!
//! One variant per current Slint `on_*` callback that mutates `session.project`.
//! Variants follow the spec's naming when the spec names them; new variants
//! use the same PascalCase, no-abbreviation convention.
//!
//! **Spec reference:** `docs/superpowers/specs/2026-04-23-command-dispatch-architecture-design.md`
//! — "Shared Architecture / Types".
//!
//! **Audit reference:** `docs/superpowers/audits/2026-05-14-command-audit.md`.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// Type aliases kept simple for the skeleton. Phase 2 tasks will import the
// canonical `domain::ids::{ChainId, BlockId}` types when the first real
// Command arm is wired end-to-end.
pub type ChainId = String;
pub type BlockId = String;

/// Every state change the UI or any controller can request.
///
/// Fine-grained: one variant per logical operation currently expressed as a
/// Slint `on_*` callback that mutates `ProjectSession.project`.
///
/// Variants are grouped by domain concern in the source file for readability;
/// the serialized form uses the variant name as-is (serde default).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum Command {
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

    /// Select an enum/option parameter by index.
    SelectBlockParameterOption {
        chain: ChainId,
        block: BlockId,
        path: String,
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

    /// Remove a block from a chain.
    RemoveBlock { chain: ChainId, block: BlockId },

    /// Move a block to `new_position` within its chain.
    MoveBlock {
        chain: ChainId,
        block: BlockId,
        new_position: usize,
    },

    // ── Block editor draft ────────────────────────────────────────────────────
    /// Flush the current block-editor draft to the project.
    ///
    /// This is a transitional command that captures the "save block drawer"
    /// operation in the current draft-based flow. Phase 1+ tasks will dissolve
    /// this into individual parameter commands once the draft indirection is
    /// removed.
    SaveBlockEditorDraft { chain: ChainId, block: BlockId },

    // ── Insert block ──────────────────────────────────────────────────────────
    /// Commit an Insert block's send/return endpoint configuration.
    SaveInsertBlock { chain: ChainId, block: BlockId },

    // ── Chain CRUD ────────────────────────────────────────────────────────────
    /// Validate and persist a chain draft (create or replace existing chain).
    SaveChain { chain: ChainId },

    /// Remove a chain from the project.
    RemoveChain { chain: ChainId },

    /// Move chain one position toward the beginning of the list.
    MoveChainUp { chain: ChainId },

    /// Move chain one position toward the end of the list.
    MoveChainDown { chain: ChainId },

    // ── Chain enable ──────────────────────────────────────────────────────────
    /// Toggle the enabled flag of a chain (starts/stops its audio runtime).
    ToggleChainEnabled { chain: ChainId },

    // ── Chain I/O endpoints ───────────────────────────────────────────────────
    /// Commit the input endpoint configuration for a chain.
    SaveChainInputEndpoints { chain: ChainId },

    /// Commit the output endpoint configuration for a chain.
    SaveChainOutputEndpoints { chain: ChainId },

    /// Commit both input and output I/O configuration for a chain
    /// (used in fullscreen I/O editor flow).
    SaveChainIo { chain: ChainId },

    // ── Chain presets ─────────────────────────────────────────────────────────
    /// Load a preset file and replace the non-I/O blocks of a chain.
    LoadChainPreset { chain: ChainId, path: PathBuf },

    // ── Project lifecycle ─────────────────────────────────────────────────────
    /// Save the project to its current path (or trigger save-as dialog).
    SaveProject,

    /// Load a project from disk, replacing the current session.
    LoadProject { path: PathBuf },

    /// Create a new project with the given name.
    CreateProject { name: String },

    // ── Project settings ──────────────────────────────────────────────────────
    /// Update the project's display name.
    UpdateProjectName { name: String },

    /// Persist the current audio device selection into the project and
    /// resync the audio runtime.
    SaveAudioSettings,
}
