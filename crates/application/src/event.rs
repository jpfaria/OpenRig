//! Typed `Event` enum — every observable change emitted by the dispatcher.
//!
//! Variants mirror the spec's "Shared Architecture / Types" section.
//!
//! **Spec reference:** `docs/superpowers/specs/2026-04-23-command-dispatch-architecture-design.md`

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::{BlockId, ChainId};

/// #553: which per-stem control was touched. Used inside
/// [`Event::StemControlChanged`] so subscribers don't need to inspect
/// the dispatcher to know what changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum StemControl {
    /// Mute toggled.
    Mute(bool),
    /// Solo toggled.
    Solo(bool),
    /// Linear gain set.
    Gain(f32),
    /// Pan set in `[-1.0, 1.0]`.
    Pan(f32),
}

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
    /// An insert block's send/return endpoints were saved.
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

    /// #553: stem separation enqueued. Adapter listens, spawns the
    /// off-RT worker, and later reports completion / failure via
    /// dedicated track events.
    StemJobQueued {
        /// Source audio file whose separation was enqueued.
        source_path: std::path::PathBuf,
    },

    /// #553: a track was requested for playback. Adapter loads the
    /// stems and hands them to the engine.
    TrackLoadRequested {
        /// Stable track id.
        track_id: String,
    },

    /// #553: the current track was released.
    TrackUnloaded,

    /// #553: a track was renamed; meta.yaml has been persisted by the
    /// adapter.
    TrackRenamed {
        /// Stable track id.
        track_id: String,
        /// Title now stored on disk.
        new_title: String,
    },

    /// #553: a track was removed from the catalog.
    TrackDeleted {
        /// Stable track id.
        track_id: String,
    },

    /// #553: playback transport intent — adapter forwards to the engine.
    TrackPlayRequested,

    /// #553: playback transport intent — adapter forwards to the engine.
    TrackPauseRequested,

    /// #553: seek intent — adapter forwards to the engine playhead.
    TrackSeekRequested {
        /// Target position in seconds.
        secs: f64,
    },

    /// #553: per-stem control intent. Adapter applies to the engine
    /// player (`MultiStemPlayer::set_*`).
    StemControlChanged {
        /// Stem index (0=drums, 1=bass, 2=vocals, 3=other).
        stem_index: usize,
        /// Which control was touched.
        kind: StemControl,
    },

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
            | Event::BlockSelectionChanged { chain, .. } => Some(chain),
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
            | Event::StemJobQueued { .. }
            | Event::TrackLoadRequested { .. }
            | Event::TrackUnloaded
            | Event::TrackRenamed { .. }
            | Event::TrackDeleted { .. }
            | Event::TrackPlayRequested
            | Event::TrackPauseRequested
            | Event::TrackSeekRequested { .. }
            | Event::StemControlChanged { .. }
            // #561: catalog-wide reload, never tied to a single chain.
            | Event::PluginCatalogReloaded { .. }
            // #561 (expanded scope): per-plugin load/unload, also catalog-scope.
            | Event::PluginLoaded { .. }
            | Event::PluginUnloaded { .. }
            // #576: offline render does not touch any chain in the live project.
            | Event::RenderCompleted { .. }
            | Event::Error { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_accessor_returns_the_affected_chain() {
        // The MIDI/MCP refresh needs to know which chain each event
        // touched so it can re-sync that chain's live runtime.
        let c = ChainId("rig:guitar".into());
        assert_eq!(Event::ChainReloaded { chain: c.clone() }.chain(), Some(&c));
        assert_eq!(
            Event::ChainVolumeChanged {
                chain: c.clone(),
                value: 80.0
            }
            .chain(),
            Some(&c)
        );
        // Project-wide events carry no chain.
        assert_eq!(Event::ProjectSaved.chain(), None);
        assert_eq!(Event::ProjectMutated.chain(), None);
    }
}
