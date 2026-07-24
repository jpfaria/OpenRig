//! Chain-scoped commands: chain CRUD and ordering, enable, I/O endpoints,
//! presets, volume/bindings, the per-chain virtual DI loop, and offline render.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use domain::ids::ChainId;
use project::chain::{Chain, DiOutputRef};

use crate::di_loader::DiLoopSource;

/// Every state change scoped to a whole chain.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ChainCommand {
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
    SetChainDiLoopEnabled { chain: ChainId, enabled: bool },

    /// #717 Task 3: persist the chosen DI output endpoint for a chain.
    ///
    /// Sets `chain.di_output = Some(output)` on the matching chain in the
    /// project and emits `Event::ChainDiLoopOutputChanged { chain }`.
    ///
    /// Returns `Err` if `chain` is not found.
    SetChainDiLoopOutput { chain: ChainId, output: DiOutputRef },

    // ── Offline render (#576) ─────────────────────────────────────────────────
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
}
