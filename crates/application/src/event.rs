//! Typed `Event` enum — every observable change emitted by the dispatcher.
//!
//! Variants mirror the spec's "Shared Architecture / Types" section.
//!
//! **Spec reference:** `docs/superpowers/specs/2026-04-23-command-dispatch-architecture-design.md`

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::{BlockId, ChainId};

/// Every observable change emitted by a [`crate::dispatcher::CommandDispatcher`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Event {
    /// The project has been mutated in some way (coarse-grained notification).
    /// Subscribers that need to fully re-render can react to this.
    ProjectMutated,

    /// The entire chain was rebuilt (e.g. blocks reordered, preset loaded).
    ChainReloaded {
        chain: ChainId,
    },

    /// A single block parameter was changed.
    BlockParameterChanged {
        chain: ChainId,
        block: BlockId,
        path: String,
    },

    /// A block's enabled state was toggled.
    BlockEnabledChanged {
        chain: ChainId,
        block: BlockId,
        enabled: bool,
    },

    /// A block's model was replaced.
    BlockReplaced {
        chain: ChainId,
        block: BlockId,
    },

    /// A new block was added.
    BlockAdded {
        chain: ChainId,
        block: BlockId,
    },

    /// A block was removed.
    BlockRemoved {
        chain: ChainId,
        block: BlockId,
    },

    /// An audio device changed (input or output selection mutated).
    DeviceChanged {
        chain: ChainId,
        block: BlockId,
    },

    // ── Chain-level events ────────────────────────────────────────────────────
    /// A new chain was added to the project.
    ChainAdded {
        chain: ChainId,
    },

    /// A chain was removed from the project.
    ChainRemoved {
        chain: ChainId,
    },

    /// A chain's enabled state was changed.
    ChainEnabledChanged {
        chain: ChainId,
        enabled: bool,
    },

    /// A chain was moved to a new position in the list.
    ChainMoved {
        chain: ChainId,
        new_position: usize,
    },

    /// A chain's metadata (name, instrument, I/O) was updated.
    ChainConfigured {
        chain: ChainId,
    },

    // ── Chain save events ─────────────────────────────────────────────────────
    /// A chain was saved (created or replaced) via the chain editor.
    ChainSaved {
        chain: ChainId,
    },

    /// A chain's input endpoints were saved.
    ChainInputEndpointsSaved {
        chain: ChainId,
    },

    /// A chain's output endpoints were saved.
    ChainOutputEndpointsSaved {
        chain: ChainId,
    },

    /// A chain's combined I/O configuration was saved.
    ChainIoSaved {
        chain: ChainId,
    },

    // ── Insert block events ───────────────────────────────────────────────────
    /// An insert block's I/O binding selection was saved (#716, model A).
    InsertBlockSaved {
        chain: ChainId,
        block: BlockId,
    },

    // ── Chain preset events ───────────────────────────────────────────────────
    /// A preset was loaded into a chain (non-I/O blocks replaced).
    ChainPresetLoaded {
        chain: ChainId,
    },

    // ── Chain volume events ───────────────────────────────────────────────────
    /// A chain's output volume was changed via the slider (issue #440).
    ChainVolumeChanged {
        chain: ChainId,
        value: f32,
    },

    // ── Chain I/O binding selection (issue #716) ──────────────────────────────
    /// A chain's selected I/O bindings changed; its input/output is rediscovered
    /// from the new selection and its runtime re-synced.
    ChainIoBindingsChanged {
        chain: ChainId,
        binding_ids: Vec<String>,
    },

    // ── Audio settings events ─────────────────────────────────────────────────
    /// Audio device settings were persisted into the project.
    AudioSettingsSaved,

    // ── MIDI device / mapping / learn events (#513 / #493) ───────────────────
    /// #513: emitted after `SaveMidiDevices` updated the in-memory
    /// `GuiSystemSettings.midi_devices` snapshot. The adapter persists
    /// config.yaml on receipt.
    MidiDevicesSaved,

    /// #513 / #493: emitted after `SaveMidiMapping` mutated the project.
    MidiMappingSaved,

    /// #513 / #493: emitted after `StartMidiLearn`/`StopMidiLearn`. The
    /// adapter forwards the flag to the daemon's control channel.
    MidiLearnStarted,
    MidiLearnStopped,

    /// #513 / #493: published by the daemon while learn-mode is active
    /// for every received MIDI event (one event = one publish). The
    /// mapping editor wiring listens for this event and fills the
    /// "trigger" field of the binding being learned.
    MidiEventReceived {
        source: project::midi::Source,
    },

    /// #436 F: the UI language preference was changed (`None` = system
    /// default). The adapter performs the persistence + live i18n swap;
    /// this records that the change went through the dispatcher.
    LanguageChanged {
        language: Option<String>,
    },

    /// #436 G: the audio output mute state changed. The adapter applies
    /// it to the runtime; this records it went through the dispatcher.
    OutputMutedChanged {
        muted: bool,
    },

    /// #436 F: a recent-projects entry was removed (by list index). The
    /// adapter performs the app-config persistence; this records the
    /// change went through the dispatcher.
    RecentProjectRemoved {
        index: usize,
    },

    /// #436 F: a chain preset file was saved. The adapter does the file
    /// I/O; this records it went through the dispatcher.
    ChainPresetSaved {
        name: String,
    },

    /// #436 F: a chain preset file was deleted. The adapter does the
    /// file I/O; this records it went through the dispatcher.
    ChainPresetDeleted {
        name: String,
    },

    /// #436 H: the Tuner analyzer was powered on/off. The adapter does
    /// the build/teardown; this records it went through the dispatcher.
    TunerEnabledChanged {
        enabled: bool,
    },

    /// #436 H: the Spectrum analyzer was powered on/off. The adapter
    /// does the build/teardown; this records it went through the
    /// dispatcher.
    SpectrumEnabledChanged {
        enabled: bool,
    },

    /// #14: the metronome was started or stopped. The adapter opens or closes
    /// the metronome's dedicated output stream; the dispatcher only records
    /// that the request came through the bus.
    MetronomeEnabledChanged {
        enabled: bool,
    },

    /// #14: the tempo changed. Already clamped to the supported range — the
    /// adapter can hand this straight to the generator.
    MetronomeBpmChanged {
        bpm: f32,
    },

    /// #14: beats per bar changed, already clamped.
    MetronomeTimeSignatureChanged {
        beats_per_bar: u32,
    },

    /// #14: the subdivision changed. Parsed and validated by the dispatcher.
    MetronomeSubdivisionChanged {
        subdivision: String,
    },

    /// #14: the click level changed, already clamped to `0.0..=1.0`.
    MetronomeVolumeChanged {
        volume: f32,
    },

    /// #14: the click timbre changed. Parsed and validated by the dispatcher.
    MetronomeTimbreChanged {
        timbre: String,
    },

    /// #14: the count-in bar was turned on or off.
    MetronomeCountInChanged {
        enabled: bool,
    },

    /// #14: the metronome's output device changed. The adapter reopens the
    /// dedicated stream on the new device.
    MetronomeOutputChanged {
        device_id: Option<String>,
    },

    /// #14: the tap-tempo button was tapped. The adapter owns the tap history
    /// and dispatches the resulting `SetMetronomeBpm`.
    MetronomeTapped,

    /// #591: the compact view was toggled (MIDI slot `toggle_compact_view`
    /// → `SetCompactViewEnabled`). The adapter opens/closes the per-chain
    /// compact window for the active chain; without this event the MIDI
    /// drain had nothing to act on and the footswitch did nothing.
    CompactViewEnabledChanged {
        enabled: bool,
    },

    /// #712: the MIDI/BLE-MIDI adapter master switch (`config.yaml`
    /// `midi_enabled`) was toggled via `SetMidiEnabled`. Persisted by the
    /// dispatcher; the adapter applies it on next launch (the subsystem is
    /// wired at bootstrap), so the GUI uses this to surface a restart hint.
    MidiEnabledChanged {
        enabled: bool,
    },

    /// #712: the MCP server master switch (`config.yaml` `mcp_enabled`)
    /// was toggled via `SetMcpEnabled`. Same restart-to-apply contract as
    /// [`Event::MidiEnabledChanged`].
    McpEnabledChanged {
        enabled: bool,
    },

    // ── Project-level events ──────────────────────────────────────────────────
    /// A project was loaded from disk.
    ProjectLoaded,

    /// The project was saved to disk.
    ProjectSaved,

    /// A new project was created.
    ProjectCreated,

    /// #436 E: the project was closed (back to launcher). The adapter
    /// tears down runtime/session; this records it went through the
    /// dispatcher.
    ProjectClosed,

    /// #436 (sweep): a recent-projects entry was registered/refreshed.
    /// The adapter persists app-config; this records it went through
    /// the dispatcher.
    RecentProjectRegistered {
        path: PathBuf,
        name: String,
    },

    /// #436 (sweep): a recent-projects entry was marked invalid. The
    /// adapter persists app-config; this records it went through the
    /// dispatcher.
    RecentProjectInvalidated {
        path: PathBuf,
        reason: String,
    },

    /// #513: emitted after `SetPresetsPath` or `SetPluginsPath` updated
    /// the in-memory `AppConfig.paths` snapshot. The adapter persists
    /// `config.yaml` on receipt. System-level event per ADR 0003.
    PathsSaved,

    /// #561: emitted after `Command::ReloadPluginCatalog` re-scanned
    /// the plugin packages directories. Carries the post-reload totals
    /// so adapters can show the user what changed (GUI toast, MCP
    /// response). `total_count == native_count + disk_count`.
    PluginCatalogReloaded {
        native_count: usize,
        disk_count: usize,
        total_count: usize,
    },

    /// #561 (expanded scope): emitted after `Command::LoadPlugin`
    /// brought a single plugin into the catalog (or confirmed it was
    /// already there). `id` mirrors the request.
    PluginLoaded {
        id: String,
    },

    /// #561 (expanded scope): emitted after `Command::UnloadPlugin`
    /// dropped a single disk plugin from the catalog. `id` mirrors
    /// the request.
    PluginUnloaded {
        id: String,
    },

    /// #693: `Command::LoadPlugin` runs its root scan on its own task —
    /// a failure (unknown id, unreadable root) surfaces as this event
    /// via the async-completion poll instead of a synchronous `Err`.
    PluginLoadFailed {
        id: String,
        reason: String,
    },

    /// An error occurred while processing a command.
    Error {
        message: String,
    },

    /// #22: the per-chain block-selection pair cursor moved; `left` is
    /// the left block index of the pair. Drives the transient selection
    /// border (shown on a footswitch stimulus, fades after a timeout).
    BlockSelectionChanged {
        chain: ChainId,
        left: usize,
    },

    /// #576: `Command::RenderChain` finished writing the output WAV.
    /// The agent receives this on the MCP side serialized as JSON, so
    /// it knows where the rendered file lives and what its key audio
    /// metadata are.
    RenderCompleted {
        output_path: String,
        duration_seconds: f64,
        sample_rate: u32,
        bit_depth: u8,
    },

    // ── Per-chain virtual DI loop (#614) ──────────────────────────────────────
    /// #614: a DI loop source was loaded and decoded for a chain.
    ///
    /// Ephemeral — not persisted. The adapter-gui reacts to this event
    /// to update any UI indicator showing the active DI source.
    ChainDiLoopSourceChanged {
        chain: ChainId,
    },

    /// #614: the DI loop playback state changed for a chain.
    ///
    /// Ephemeral — not persisted. `enabled: true` means the pre-loaded
    /// `Arc<DiPcm>` source should be armed on the chain's runtimes, resampled
    /// per output-stream rate (#749); the adapter-gui wiring (Task 6) retrieves
    /// it via `LocalDispatcher::di_loop_for_chain` and forwards it to
    /// `set_chain_di_loop`. `enabled: false` disarms every runtime.
    ChainDiLoopEnabledChanged {
        chain: ChainId,
        enabled: bool,
    },

    /// #717 Task 3: the chain's chosen DI output endpoint was persisted.
    ///
    /// The adapter-gui reacts to this event to refresh any UI showing the
    /// selected DI output. The new value can be read back from the project via
    /// `chain.di_output`.
    ChainDiLoopOutputChanged {
        chain: ChainId,
    },

    // ── I/O binding registry (#716) ───────────────────────────────────────────
    /// #716: the per-machine I/O binding registry in `config.yaml` was
    /// mutated (create, update, or delete). MCP/gRPC adapters that cache
    /// the registry invalidate their cache on receipt.
    IoBindingRegistryChanged,
}

impl Event {
    /// The chain this event affected, if any. Project-wide events
    /// (`ProjectSaved`, `ProjectMutated`, …) return `None`. Used by the
    /// MIDI/MCP drain to re-sync exactly the chains a footswitch touched.
    pub fn chain(&self) -> Option<&ChainId> {
        match self {
            Event::ChainReloaded { chain }
            | Event::BlockParameterChanged { chain, .. }
            | Event::BlockEnabledChanged { chain, .. }
            | Event::BlockReplaced { chain, .. }
            | Event::BlockAdded { chain, .. }
            | Event::BlockRemoved { chain, .. }
            | Event::DeviceChanged { chain, .. }
            | Event::ChainAdded { chain }
            | Event::ChainRemoved { chain }
            | Event::ChainEnabledChanged { chain, .. }
            | Event::ChainMoved { chain, .. }
            | Event::ChainConfigured { chain }
            | Event::ChainSaved { chain }
            | Event::ChainInputEndpointsSaved { chain }
            | Event::ChainOutputEndpointsSaved { chain }
            | Event::ChainIoSaved { chain }
            | Event::InsertBlockSaved { chain, .. }
            | Event::ChainPresetLoaded { chain }
            | Event::ChainVolumeChanged { chain, .. }
            | Event::ChainIoBindingsChanged { chain, .. }
            | Event::BlockSelectionChanged { chain, .. }
            | Event::ChainDiLoopSourceChanged { chain }
            | Event::ChainDiLoopEnabledChanged { chain, .. }
            | Event::ChainDiLoopOutputChanged { chain } => Some(chain),
            Event::ProjectMutated
            | Event::AudioSettingsSaved
            | Event::ProjectLoaded
            | Event::ProjectSaved
            | Event::ProjectCreated
            // #436 F/G/H/E + sweep: app/project-wide events, no ChainId.
            | Event::LanguageChanged { .. }
            | Event::OutputMutedChanged { .. }
            | Event::RecentProjectRemoved { .. }
            | Event::RecentProjectRegistered { .. }
            | Event::RecentProjectInvalidated { .. }
            | Event::ChainPresetSaved { .. }
            | Event::ChainPresetDeleted { .. }
            | Event::TunerEnabledChanged { .. }
            | Event::SpectrumEnabledChanged { .. }
            | Event::MetronomeEnabledChanged { .. }
            | Event::MetronomeBpmChanged { .. }
            | Event::MetronomeTimeSignatureChanged { .. }
            | Event::MetronomeSubdivisionChanged { .. }
            | Event::MetronomeVolumeChanged { .. }
            | Event::MetronomeTimbreChanged { .. }
            | Event::MetronomeCountInChanged { .. }
            | Event::MetronomeOutputChanged { .. }
            | Event::MetronomeTapped
            | Event::CompactViewEnabledChanged { .. }
            | Event::MidiEnabledChanged { .. }
            | Event::McpEnabledChanged { .. }
            | Event::ProjectClosed
            // #513 / #493: MIDI device / mapping / learn events live at the
            // system or project root, not a single chain.
            | Event::MidiDevicesSaved
            | Event::MidiMappingSaved
            | Event::MidiLearnStarted
            | Event::MidiLearnStopped
            | Event::MidiEventReceived { .. }
            // #513: system-level paths event, never tied to a chain.
            | Event::PathsSaved
            // #561: catalog-wide reload, never tied to a single chain.
            | Event::PluginCatalogReloaded { .. }
            // #561 (expanded scope): per-plugin load/unload, also catalog-scope.
            | Event::PluginLoaded { .. }
            | Event::PluginUnloaded { .. }
            | Event::PluginLoadFailed { .. }
            // #576: offline render does not touch any chain in the live project.
            | Event::RenderCompleted { .. }
            | Event::Error { .. }
            // #716: I/O binding registry is a system-level concern, not tied to any chain.
            | Event::IoBindingRegistryChanged => None,
        }
    }
}

#[cfg(test)]
#[path = "event_tests.rs"]
mod tests;
