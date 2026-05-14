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

    /// A project was loaded from disk.
    ProjectLoaded,

    /// The project was saved to disk.
    ProjectSaved,

    /// An error occurred while processing a command.
    Error { message: String },
}
