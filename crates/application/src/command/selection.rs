//! Selection and view-state commands: rig navigation, the active chain/block
//! cursor MIDI and MCP can move, and the analyzer/output view toggles.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use domain::ids::ChainId;

use crate::command::RigNavKind;

/// Every state change scoped to the selection cursor or a view toggle.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum SelectionCommand {
    // ── Rig navigation ────────────────────────────────────────────────────────
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

    // ── Active chain / block cursor ───────────────────────────────────────────
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

    /// #548: toggle the block immediately AFTER the active block in
    /// the active chain (wraps to first). Backs MIDI slot
    /// `toggle_active_block_neighbor_enabled`.
    ToggleActiveBlockNeighborEnabled,

    // ── Output / analyzers ────────────────────────────────────────────────────
    /// #436 G: mute/unmute the audio output (tuner mute). Was GUI-only
    /// (`rt.set_output_muted` in a wiring closure). Now a Command so
    /// MIDI/MCP can request it too. `SaveProject` precedent: the adapter
    /// applies it to the audio runtime; the dispatcher records the
    /// intent and signals it via `Event::OutputMutedChanged`.
    SetOutputMuted { muted: bool },

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
}
