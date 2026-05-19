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

pub use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, InsertEndpoint};
use project::chain::Chain;

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
    ///
    /// **Note:** Same `schemars` caveat as `AddChain`.
    #[schemars(skip)]
    InsertPrebuiltBlock {
        chain: ChainId,
        #[schemars(skip)]
        block: AudioBlock,
        position: usize,
    },

    /// Overwrite the block with the given `block_id` in-place.
    ///
    /// The dispatcher locates the block by `block_id` and replaces it with
    /// the provided `replacement`. The replacement's `id` field is ignored —
    /// the original `block_id` is preserved on the stored block.
    ///
    /// **Note:** Same `schemars` caveat as `AddChain`.
    #[schemars(skip)]
    OverwriteBlock {
        chain: ChainId,
        block: BlockId,
        #[schemars(skip)]
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
    /// Commit an Insert block's send/return endpoint configuration.
    ///
    /// The caller supplies the fully-resolved `send` and `return_` endpoints.
    /// The dispatcher locates the block and replaces its `InsertBlock` data.
    #[schemars(skip)]
    SaveInsertBlock {
        chain: ChainId,
        block: BlockId,
        #[schemars(skip)]
        send: InsertEndpoint,
        #[schemars(skip)]
        return_: InsertEndpoint,
    },

    // ── Chain CRUD ────────────────────────────────────────────────────────────
    /// Add a fully-constructed chain to the project.
    ///
    /// The caller is responsible for building the chain (including I/O blocks)
    /// before dispatching. Use `chain_factory::build_default_chain` as the
    /// starting point.
    ///
    /// **Note:** `Chain` does not currently implement `JsonSchema` (it lacks
    /// the `schemars` dependency in the `project` crate). The schema field is
    /// skipped here; MCP/gRPC adapters that need the schema will need to add
    /// `schemars` to `project` in a later task.
    #[schemars(skip)]
    AddChain {
        #[schemars(skip)]
        chain: Chain,
    },

    /// Replace an existing chain's metadata and I/O configuration.
    ///
    /// The caller supplies the fully-updated chain (preserving the original
    /// `chain.id` so the dispatcher can locate and replace it).
    ///
    /// **Note:** Same `schemars` caveat as `AddChain`.
    #[schemars(skip)]
    ConfigureChain {
        #[schemars(skip)]
        chain: Chain,
    },

    /// Validate and persist a chain draft (create or replace existing chain).
    ///
    /// The caller supplies the fully-constructed chain. The dispatcher uses
    /// `chain.id` to locate the existing entry and replace it in-place, or
    /// appends the chain when no existing entry with the same id is found.
    ///
    /// **Note:** Same `schemars` caveat as `AddChain`.
    #[schemars(skip)]
    SaveChain {
        #[schemars(skip)]
        chain: Chain,
    },

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
    ///
    /// The caller supplies the fully-constructed list of `InputBlock`s to
    /// replace the existing ones. The dispatcher locates the chain, removes
    /// all existing `InputBlock` entries, inserts the provided blocks at the
    /// head of the chain (inputs-first convention), and emits
    /// `ChainInputEndpointsSaved`. An empty `input_blocks` vec clears all
    /// inputs.
    #[schemars(skip)]
    SaveChainInputEndpoints {
        chain: ChainId,
        #[schemars(skip)]
        input_blocks: Vec<AudioBlock>,
    },

    /// Commit the output endpoint configuration for a chain.
    ///
    /// Same pattern as `SaveChainInputEndpoints` but for the output side.
    /// The dispatcher removes all existing `OutputBlock` entries and appends
    /// the provided blocks at the tail of the chain.
    #[schemars(skip)]
    SaveChainOutputEndpoints {
        chain: ChainId,
        #[schemars(skip)]
        output_blocks: Vec<AudioBlock>,
    },

    /// Commit both input and output I/O configuration for a chain
    /// (used in fullscreen I/O editor flow).
    ///
    /// The caller supplies both the updated `InputBlock` and `OutputBlock`.
    #[schemars(skip)]
    SaveChainIo {
        chain: ChainId,
        #[schemars(skip)]
        input_block: AudioBlock,
        #[schemars(skip)]
        output_block: AudioBlock,
    },

    // ── Chain presets ─────────────────────────────────────────────────────────
    /// Replace the non-I/O blocks of a chain with the supplied preset blocks.
    ///
    /// File I/O (YAML parsing) is done in the adapter before dispatching. The
    /// adapter passes the fully-parsed, I/O-stripped list of blocks. The
    /// dispatcher replaces `chain.blocks` and emits `ChainPresetLoaded`.
    #[schemars(skip)]
    LoadChainPreset {
        chain: ChainId,
        #[schemars(skip)]
        preset_blocks: Vec<project::block::AudioBlock>,
    },

    // ── Project lifecycle ─────────────────────────────────────────────────────
    /// Save the project to its current path (or trigger save-as dialog).
    ///
    /// File I/O happens in the adapter before this command is dispatched. The
    /// dispatcher emits `ProjectSaved` to notify subscribers.
    SaveProject,

    /// Load a project from disk, replacing the current session.
    ///
    /// The adapter performs YAML parsing and constructs the `Project` before
    /// dispatching. The dispatcher replaces the shared project handle contents
    /// with the provided project and emits `ProjectLoaded { path }`.
    /// `path` is carried only for the event payload (not for I/O).
    #[schemars(skip)]
    LoadProject {
        #[schemars(skip)]
        project: project::project::Project,
        path: PathBuf,
    },

    /// Create a new project with the given name, replacing the current session.
    ///
    /// The adapter constructs the new empty `Project` before dispatching. The
    /// dispatcher replaces the shared project handle and emits `ProjectCreated`.
    #[schemars(skip)]
    CreateProject {
        #[schemars(skip)]
        project: project::project::Project,
    },

    // ── Chain volume ──────────────────────────────────────────────────────────
    /// Set the output volume of a chain (issue #440).
    ///
    /// `value` is the volume in percent (100 = unity, 200 = +6 dB, 50 = -6 dB).
    /// No clamping is applied — the caller is responsible for keeping `value`
    /// within a sane range. The engine multiplies the master output by
    /// `value / 100` on every audio callback.
    SetChainVolume { chain: ChainId, value: f32 },

    // ── Project settings ──────────────────────────────────────────────────────
    /// Update the project's display name.
    UpdateProjectName { name: String },

    /// Persist the current audio device selection into the project and
    /// resync the audio runtime.
    ///
    /// The adapter collects the selected device rows and resolves them to
    /// `DeviceSettings` before dispatching. The dispatcher replaces the
    /// project's `device_settings` with the provided list.
    #[schemars(skip)]
    SaveAudioSettings {
        #[schemars(skip)]
        device_settings: Vec<project::device::DeviceSettings>,
    },

    /// #436: per-chain rig navigation (preset/scene switch/add/remove).
    /// The GUI used to mutate `RigProject` by hand in a wiring closure —
    /// business logic in the UI. Now it dispatches this and the
    /// dispatcher (which owns the rig) re-projects the synthetic chain.
    /// `kind` carries the GUI's existing sentinel int (≥0 select, -1
    /// add, -2 remove) so no new behaviour is introduced.
    ApplyRigNav { chain: ChainId, kind: RigNavKind },

    /// #436: rename the chain's ACTIVE rig preset (the human `name`
    /// shown in the select). The UI just dispatches this; the
    /// dispatcher (owning the rig) writes `RigPreset.name`.
    RenameRigPreset { chain: ChainId, name: String },

    /// #436: select a block on a chain (the cursor MIDI/MCP can move).
    /// Was GUI-only state; now dispatcher-owned so it is reachable.
    SelectChainBlock { chain: ChainId, block_index: usize },

    /// #436: capture pending edits on the projected synthetic chains
    /// back into the rig. The GUI save path used to call
    /// `sync_synthetic_into_rig` by hand (model mutation in the UI);
    /// it now dispatches this so the dispatcher owns the mutation.
    CaptureRigEdits,

    /// #436 F: set the UI language preference. Was GUI-only
    /// (`FilesystemStorage::save_gui_language` + live i18n swap in a
    /// wiring closure). Now a Command so MIDI/MCP can request it too.
    /// Follows the `SaveProject` precedent: the adapter performs the
    /// persistence + live swap; the dispatcher records the intent and
    /// signals it via `Event::LanguageChanged`. `None` = system default.
    SetLanguage { language: Option<String> },

    /// #436 G: mute/unmute the audio output (tuner mute). Was GUI-only
    /// (`rt.set_output_muted` in a wiring closure). Now a Command so
    /// MIDI/MCP can request it too. `SaveProject` precedent: the adapter
    /// applies it to the audio runtime; the dispatcher records the
    /// intent and signals it via `Event::OutputMutedChanged`.
    SetOutputMuted { muted: bool },

    /// #436 F: remove an entry from the recent-projects list (persisted
    /// app-config preference). Was GUI-only (`save_app_config` in a
    /// wiring closure). Now a Command so MIDI/MCP can request it too.
    /// `SaveProject` precedent: the adapter performs the persistence;
    /// the dispatcher records the intent and signals it via
    /// `Event::RecentProjectRemoved`.
    RemoveRecentProject { index: usize },

    /// #436 F: save the active chain as a named preset file. Was
    /// GUI-only (direct file write in a wiring closure). `SaveProject`
    /// precedent: the adapter writes the file; the dispatcher records
    /// the intent and signals `Event::ChainPresetSaved`.
    SaveChainPreset { name: String },

    /// #436 F: delete a named chain preset file. Was GUI-only
    /// (`std::fs::remove_file` in a wiring closure). `SaveProject`
    /// precedent: the adapter removes the file; the dispatcher records
    /// the intent and signals `Event::ChainPresetDeleted`.
    DeleteChainPreset { name: String },

    /// #436 H: power the Tuner analyzer on/off. Was GUI-only (build/
    /// teardown of the analysis session + timer + runtime in a wiring
    /// closure). `SaveProject` precedent: the adapter does the build/
    /// teardown; the dispatcher records the intent and signals
    /// `Event::TunerEnabledChanged`.
    SetTunerEnabled { enabled: bool },

    /// #436 H: power the Spectrum analyzer on/off. Same shape as
    /// `SetTunerEnabled`; adapter does the build/teardown, dispatcher
    /// signals `Event::SpectrumEnabledChanged`.
    SetSpectrumEnabled { enabled: bool },

    /// #436 E: close the current project (back to launcher). Was
    /// GUI-only (stop runtime + drop session in a wiring closure).
    /// `SaveProject` precedent: the adapter tears down the runtime/
    /// session; the dispatcher records the intent and signals
    /// `Event::ProjectClosed`.
    CloseProject,
}

/// What [`Command::ApplyRigNav`] does to the chain's rig input. The
/// `i32` is the existing GUI sentinel: `>= 0` selects that
/// preset-position / scene number, `-1` adds, `-2` removes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum RigNavKind {
    Preset(i32),
    Scene(i32),
}
