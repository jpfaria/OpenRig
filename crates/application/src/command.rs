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
pub use domain::io_binding::{ChannelMode, IoBinding};
use project::block::AudioBlock;
use project::chain::Chain;
pub use crate::di_loader::DiLoopSource;

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

    // ── Chain CRUD ────────────────────────────────────────────────────────────
    /// Add a fully-constructed chain to the project.
    ///
    /// The caller is responsible for building the chain (including I/O blocks)
    /// before dispatching. Use `chain_factory::build_default_chain` as the
    /// starting point.
    ///
    AddChain { chain: Chain },

    /// Replace an existing chain's metadata and I/O configuration.
    ///
    /// The caller supplies the fully-updated chain (preserving the original
    /// `chain.id` so the dispatcher can locate and replace it).
    ConfigureChain { chain: Chain },

    /// Validate and persist a chain draft (create or replace existing chain).
    ///
    /// The caller supplies the fully-constructed chain. The dispatcher uses
    /// `chain.id` to locate the existing entry and replace it in-place, or
    /// appends the chain when no existing entry with the same id is found.
    SaveChain { chain: Chain },

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
    /// Bind the input block at `block_index` in the named chain to an I/O
    /// binding reference.
    ///
    /// The dispatcher locates the chain, finds the input block at
    /// `block_index`, and sets `block.io = io` and `block.endpoint = endpoint`.
    /// Emits `ChainInputEndpointsSaved`. Returns `Err` when the chain or the
    /// block index is not found, or when the target block is not an
    /// `InputBlock`.
    SaveChainInputEndpoints {
        chain: ChainId,
        block_index: usize,
        io: String,
        endpoint: String,
    },

    /// Bind the output block at `block_index` in the named chain to an I/O
    /// binding reference.
    ///
    /// Same semantics as `SaveChainInputEndpoints` but for output blocks.
    /// Emits `ChainOutputEndpointsSaved`.
    SaveChainOutputEndpoints {
        chain: ChainId,
        block_index: usize,
        io: String,
        endpoint: String,
    },

    /// Bind both the input block at `input_block_index` and the output block
    /// at `output_block_index` in the named chain to the same I/O binding
    /// reference (used in the fullscreen I/O editor flow).
    ///
    /// Emits both `ChainInputEndpointsSaved` and `ChainOutputEndpointsSaved`.
    SaveChainIo {
        chain: ChainId,
        input_block_index: usize,
        output_block_index: usize,
        io: String,
        endpoint: String,
    },

    // ── Chain presets ─────────────────────────────────────────────────────────
    /// Replace the non-I/O blocks of a chain with the supplied preset blocks.
    ///
    /// File I/O (YAML parsing) is done in the adapter before dispatching. The
    /// adapter passes the fully-parsed, I/O-stripped list of blocks. The
    /// dispatcher replaces `chain.blocks` and emits `ChainPresetLoaded`.
    ///
    /// `preset_instrument` is the instrument tag read from the preset file
    /// (defaults to "electric_guitar" for untagged legacy files). The
    /// dispatcher rejects the load if it differs from the target chain's
    /// instrument.
    LoadChainPreset {
        chain: ChainId,
        /// Instrument tag from the preset file. Use "electric_guitar" for
        /// untagged legacy presets.
        preset_instrument: String,
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
    LoadProject {
        project: project::project::Project,
        path: PathBuf,
    },

    /// Create a new project with the given name, replacing the current session.
    ///
    /// The adapter constructs the new empty `Project` before dispatching. The
    /// dispatcher replaces the shared project handle and emits `ProjectCreated`.
    CreateProject { project: project::project::Project },

    // ── Chain volume ──────────────────────────────────────────────────────────
    /// Set the output volume of a chain (issue #440).
    ///
    /// `value` is the volume in percent (100 = unity, 200 = +6 dB, 50 = -6 dB).
    /// No clamping is applied — the caller is responsible for keeping `value`
    /// within a sane range. The engine multiplies the master output by
    /// `value / 100` on every audio callback.
    SetChainVolume { chain: ChainId, value: f32 },

    /// Set the I/O bindings a chain uses (issue #716). `binding_ids` is the full
    /// selection (the checklist sends its entire set); the chain's input/output
    /// is discovered from these bindings. Replaces any previous selection.
    SetChainIoBindings {
        chain: ChainId,
        binding_ids: Vec<String>,
    },

    // ── Project settings ──────────────────────────────────────────────────────
    /// Update the project's display name.
    UpdateProjectName { name: String },

    /// Persist the current audio device selection into the project and
    /// resync the audio runtime.
    ///
    /// The adapter collects the selected device rows and resolves them to
    /// `DeviceSettings` before dispatching. The dispatcher replaces the
    /// project's `device_settings` with the provided list.
    SaveAudioSettings {
        /// Devices selected as inputs. Persisted into `config.input_devices`.
        input_devices: Vec<project::device::DeviceSettings>,
        /// Devices selected as outputs. Persisted into `config.output_devices`.
        ///
        /// Kept separate from `input_devices` because the same physical
        /// interface enumerates with a different `device_id` per direction
        /// (CoreAudio/WASAPI); collapsing both into one flat list corrupts the
        /// saved selection and breaks re-match on reopen (#581 follow-up).
        output_devices: Vec<project::device::DeviceSettings>,
    },

    /// #513: persist the per-machine MIDI device selection (config.yaml).
    /// The dispatcher emits `MidiDevicesSaved` only; persistence happens
    /// in the adapter wiring, identical to `SaveAudioSettings`'s system-
    /// side counterpart (no project mutation here — MIDI devices are a
    /// system-level concept per ADR 0003).
    SaveMidiDevices {
        devices: Vec<infra_filesystem::MidiDeviceSelection>,
    },

    /// #513 / #493: replace the project's MIDI binding list. Writes
    /// `project.midi.bindings`. The adapter persists the project file
    /// after `Event::MidiMappingSaved` fans out.
    SaveMidiMapping {
        bindings: Vec<project::midi::Binding>,
    },

    /// #513 / #493: put the MIDI daemon into single-shot learn mode. The
    /// next received MIDI event is published as `MidiEventReceived` and
    /// the daemon returns to normal mode automatically.
    StartMidiLearn,

    /// #513 / #493: cancel an outstanding learn request (the user closed
    /// the editor or pressed Cancel before any event arrived).
    StopMidiLearn,

    /// #513 / #493: emitted by the MIDI daemon while learn-mode is active.
    /// The daemon submits this through the existing command bridge (#165
    /// / #22) instead of routing the event itself, so the event still
    /// reaches the GUI through `PublishingDispatcher`'s fan-out — one
    /// transport, one ordering invariant. The handler is a pure passthrough.
    PublishMidiEvent { source: project::midi::Source },

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

    /// #591: select a whole chain as the active one (no specific block).
    /// Dispatched when the user taps a chain on the Chains screen so the
    /// footswitch slot `toggle_active_chain_enabled` (which reads
    /// `SelectionState.active_chain`) follows the on-screen selection.
    /// Clears the active block — a block belongs to one chain. Errors if
    /// the chain does not exist.
    SelectActiveChain { chain: ChainId },

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

    /// #555: save a chain's current FX blocks as a named preset file.
    /// The dispatcher snapshots `project.chains[chain]`, strips
    /// input/output blocks (I/O wiring isn't part of a preset), and
    /// writes the YAML under the configured `presets_path`. Every
    /// transport (GUI / MCP / gRPC) dispatches the same Command and
    /// gets the same on-disk effect.
    SaveChainPreset { chain: ChainId, name: String },

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

    /// #436 (sweep): register/refresh a recent-projects entry. Was
    /// GUI-only (`register_recent_project` + `save_app_config` in
    /// open/save closures). `SaveProject` precedent: the adapter
    /// persists app-config; the dispatcher records the intent and
    /// signals `Event::RecentProjectRegistered`.
    RegisterRecentProject { path: PathBuf, name: String },

    /// #436 (sweep): mark a recent-projects entry invalid (failed open).
    /// Same precedent; signals `Event::RecentProjectInvalidated`.
    MarkRecentProjectInvalid { path: PathBuf, reason: String },

    /// #513: persist the user's preferred directory for project preset
    /// libraries. `None` resets to the OS default (the existing resolver
    /// wins again). System-level setting per ADR 0003 — the adapter
    /// writes it into `config.yaml` on `Event::PathsSaved`.
    SetPresetsPath { path: Option<PathBuf> },

    /// #513: persist the user's preferred directory for plugin scanning
    /// (NAM/IR/LV2 packs). `None` resets to the OS default. System-level
    /// setting per ADR 0003 — the adapter writes it into `config.yaml`
    /// on `Event::PathsSaved`.
    SetPluginsPath { path: Option<PathBuf> },

    /// #582: persist the user's preferred directory for evaluation
    /// artifacts (tone-analyzer outputs, fingerprints, comparison
    /// reports). `None` resets to the OS default
    /// ([`infra_filesystem::default_evaluations_path`]). System-level
    /// setting per ADR 0003 — the adapter writes it into `config.yaml`
    /// on `Event::PathsSaved`.
    SetEvaluationsPath { path: Option<PathBuf> },

    /// #561: re-scan the plugin packages directories without restarting
    /// the process. Same path resolution as boot
    /// (`detect_data_root().join("plugins")` + `plugins_root_from_config`),
    /// natives are preserved. The dispatcher emits
    /// `Event::PluginCatalogReloaded { native_count, disk_count,
    /// total_count }` so adapters can surface the new totals to the
    /// user (GUI toast, MCP tool response). Closes the gap between
    /// "import a new NAM" and "build a preset that uses it" without a
    /// session break.
    ReloadPluginCatalog,

    /// #561 (expanded scope): bring a single disk plugin into the
    /// in-memory catalog by manifest id. Re-scans the known plugin
    /// roots and adds the one whose id matches. Errors when no disk
    /// package with that id is discoverable; no-op when the plugin
    /// is already loaded. Emits `Event::PluginLoaded { id }`.
    LoadPlugin { id: String },

    /// #561 (expanded scope): remove a single disk plugin from the
    /// in-memory catalog by manifest id. Refuses natives — they are
    /// compiled-in and cannot be dropped without restarting the
    /// process. Emits `Event::PluginUnloaded { id }`.
    UnloadPlugin { id: String },

    /// #548: move the GUI's active chain selection by `delta` positions
    /// (wraps). Backs MIDI slots `prev_chain` / `next_chain`. Mutates
    /// `SelectionState::active_chain` and clears `active_block` (block
    /// belongs to chain).
    SelectActiveChainRelative { delta: i32 },

    /// #548: move the GUI's active block selection by `delta` positions
    /// inside the active chain (wraps, skipping Input/Output blocks).
    SelectActiveBlockRelative { delta: i32 },

    /// #548: toggle the compact-view UI mode for the active chain.
    SetCompactViewEnabled { enabled: bool },

    /// #712: master switch for the MIDI/BLE-MIDI adapter, persisted into
    /// the per-machine `config.yaml` (`midi_enabled`). Set from the
    /// Settings toggle so packaged builds — launched with no CLI flags —
    /// can bring MIDI up. Per-machine (ADR 0003), distinct from the
    /// per-port `midi_devices[].enabled` selection. Takes effect on next
    /// launch (the adapter is wired at bootstrap).
    SetMidiEnabled { enabled: bool },

    /// #712: master switch for the MCP server, persisted into
    /// `config.yaml` (`mcp_enabled`). Same per-machine, restart-to-apply
    /// contract as [`Command::SetMidiEnabled`].
    SetMcpEnabled { enabled: bool },

    /// #548: toggle the block immediately AFTER the active block in
    /// the active chain (wraps to first). Backs MIDI slot
    /// `toggle_active_block_neighbor_enabled`.
    ToggleActiveBlockNeighborEnabled,

    /// #576: headless offline render — apply a chain/preset YAML to an
    /// input WAV and write the processed output WAV. File mode only;
    /// the `openrig-render` binary owns the live-capture convenience
    /// (cpal-driven) so `application` stays free of audio-device deps.
    ///
    /// Does NOT mutate the project's State. It lives on the Command
    /// bus so MCP/gRPC/any future transport adapter inherits the tool
    /// through `command_schema` instead of each adapter wiring it
    /// manually (LEI: every user operation is a Command, transports
    /// inherit parity automatically).
    RenderChain {
        chain_path: String,
        input_path: String,
        output_path: String,
        start_s: Option<f32>,
        end_s: Option<f32>,
        sample_rate_hz: Option<u32>,
        block_size: Option<u32>,
        bit_depth: Option<u8>,
        tail_ms: Option<u32>,
    },

    // ── Per-chain virtual DI loop (#614) ──────────────────────────────────────
    /// #614: load and pre-decode a DI loop source for a chain.
    ///
    /// **EPHEMERAL — never serialized into the project** (distinct from any
    /// project-level DI configuration in #324). The dispatcher decodes the
    /// source off the audio thread, stores the resulting `Arc<DiPcm>` (the
    /// un-resampled source; #749 resamples per output rate at arm time) keyed
    /// by `chain` in an in-memory map, and emits
    /// `Event::ChainDiLoopSourceChanged`. The chain's audio thread is NOT
    /// touched here — call `SetChainDiLoopEnabled { enabled: true }` to start
    /// playback.
    ///
    /// Returns `Err` if the file cannot be decoded (never silently swallows
    /// a decode failure). Returns `Err` if `chain` is not found.
    SetChainDiLoopSource {
        chain: ChainId,
        source: DiLoopSource,
    },

    /// #614: start or stop DI loop playback on a chain.
    ///
    /// **EPHEMERAL — never serialized into the project**.
    ///
    /// `enabled: true` — publishes the pre-loaded `Arc<DiPcm>` via
    /// `Event::ChainDiLoopEnabledChanged { chain, enabled: true }`.
    /// The adapter-gui wiring (Task 6) reacts to this event and arms the
    /// chain's runtimes (resampling per output rate). If no DI loop has been loaded for
    /// `chain` yet this is a no-op (emits the event with `enabled: true`
    /// so the adapter can decide).
    ///
    /// `enabled: false` — emits `Event::ChainDiLoopEnabledChanged { chain,
    /// enabled: false }`. The adapter-gui wiring calls
    /// `runtime.set_di_loop(None)`.
    ///
    /// Returns `Err` if `chain` is not found.
    SetChainDiLoopEnabled {
        chain: ChainId,
        enabled: bool,
    },

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

/// What [`Command::ApplyRigNav`] does to the chain's rig input.
///
/// `Preset`/`Scene` carry the GUI sentinel `i32`: `>= 0` selects that
/// preset-position / scene number, `-1` adds, `-2` removes.
///
/// `StepPreset`/`StepScene` carry a relative delta (`+1` next, `-1`
/// previous) and wrap — a footswitch has no absolute position, it just
/// advances. The dispatcher resolves the delta against the live rig.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum RigNavKind {
    Preset(i32),
    Scene(i32),
    StepPreset(i32),
    StepScene(i32),
}
