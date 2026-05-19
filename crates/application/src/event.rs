//! Typed `Event` enum — every observable change emitted by the dispatcher.
//!
//! Variants mirror the spec's "Shared Architecture / Types" section.
//!
//! **Spec reference:** `docs/superpowers/specs/2026-04-23-command-dispatch-architecture-design.md`

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::{BlockId, ChainId};

/// Every observable change emitted by a [`crate::dispatcher::CommandDispatcher`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum Event {
    /// The project has been mutated in some way (coarse-grained notification).
    /// Subscribers that need to fully re-render can react to this.
    ProjectMutated,

    /// The entire chain was rebuilt (e.g. blocks reordered, preset loaded).
    ChainReloaded { chain: ChainId },

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
    BlockReplaced { chain: ChainId, block: BlockId },

    /// A new block was added.
    BlockAdded { chain: ChainId, block: BlockId },

    /// A block was removed.
    BlockRemoved { chain: ChainId, block: BlockId },

    /// An audio device changed (input or output selection mutated).
    DeviceChanged { chain: ChainId, block: BlockId },

    // ── Chain-level events ────────────────────────────────────────────────────
    /// A new chain was added to the project.
    ChainAdded { chain: ChainId },

    /// A chain was removed from the project.
    ChainRemoved { chain: ChainId },

    /// A chain's enabled state was changed.
    ChainEnabledChanged { chain: ChainId, enabled: bool },

    /// A chain was moved to a new position in the list.
    ChainMoved { chain: ChainId, new_position: usize },

    /// A chain's metadata (name, instrument, I/O) was updated.
    ChainConfigured { chain: ChainId },

    // ── Chain save events ─────────────────────────────────────────────────────
    /// A chain was saved (created or replaced) via the chain editor.
    ChainSaved { chain: ChainId },

    /// A chain's input endpoints were saved.
    ChainInputEndpointsSaved { chain: ChainId },

    /// A chain's output endpoints were saved.
    ChainOutputEndpointsSaved { chain: ChainId },

    /// A chain's combined I/O configuration was saved.
    ChainIoSaved { chain: ChainId },

    // ── Insert block events ───────────────────────────────────────────────────
    /// An insert block's send/return endpoints were saved.
    InsertBlockSaved { chain: ChainId, block: BlockId },

    // ── Chain preset events ───────────────────────────────────────────────────
    /// A preset was loaded into a chain (non-I/O blocks replaced).
    ChainPresetLoaded { chain: ChainId },

    // ── Chain volume events ───────────────────────────────────────────────────
    /// A chain's output volume was changed via the slider (issue #440).
    ChainVolumeChanged { chain: ChainId, value: f32 },

    // ── Audio settings events ─────────────────────────────────────────────────
    /// Audio device settings were persisted into the project.
    AudioSettingsSaved,

    /// #436 F: the UI language preference was changed (`None` = system
    /// default). The adapter performs the persistence + live i18n swap;
    /// this records that the change went through the dispatcher.
    LanguageChanged { language: Option<String> },

    /// #436 G: the audio output mute state changed. The adapter applies
    /// it to the runtime; this records it went through the dispatcher.
    OutputMutedChanged { muted: bool },

    /// #436 F: a recent-projects entry was removed (by list index). The
    /// adapter performs the app-config persistence; this records the
    /// change went through the dispatcher.
    RecentProjectRemoved { index: usize },

    /// #436 F: a chain preset file was saved. The adapter does the file
    /// I/O; this records it went through the dispatcher.
    ChainPresetSaved { name: String },

    /// #436 F: a chain preset file was deleted. The adapter does the
    /// file I/O; this records it went through the dispatcher.
    ChainPresetDeleted { name: String },

    /// #436 H: the Tuner analyzer was powered on/off. The adapter does
    /// the build/teardown; this records it went through the dispatcher.
    TunerEnabledChanged { enabled: bool },

    /// #436 H: the Spectrum analyzer was powered on/off. The adapter
    /// does the build/teardown; this records it went through the
    /// dispatcher.
    SpectrumEnabledChanged { enabled: bool },

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

    /// An error occurred while processing a command.
    Error { message: String },
}
