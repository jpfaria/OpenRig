//! Per-chain looper commands (#323): membership, transport, the persisted
//! parameters, the record/playback endpoints and the linked preset.
//!
//! Chain-scoped, but split into their own group because they have a dedicated
//! runtime wiring (the controller-owned looper store) and a dedicated handler —
//! mirroring how the metronome and Tone Doctor got their own leaves.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use domain::ids::ChainId;
use project::chain::EndpointRef;

use crate::command::{LooperAction, LooperParam};
use crate::looper_edit::LoopEdit;

/// Every per-chain looper state change any controller (GUI, MIDI, MCP) can
/// request. The transport carries no project state (a recording is runtime
/// state); the adapter wiring turns the emitted event into the store op.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum LooperCommand {
    /// #323: add a looper to a chain. The dispatcher assigns its uid (unique
    /// within the chain, never 0) and emits `Event::ChainLooperAdded` carrying
    /// it. Fails when the chain already holds `LOOPER_MAX_PER_CHAIN` loopers.
    AddChainLooper { chain: ChainId },

    /// #323: remove a looper from a chain, discarding whatever it recorded.
    /// Fails when the chain or the looper is unknown.
    RemoveChainLooper { chain: ChainId, looper: u64 },

    /// #323: drive one looper's transport — the same command a GUI button, a
    /// MIDI footswitch and an MCP tool all go through.
    SetChainLooperTransport {
        chain: ChainId,
        looper: u64,
        action: LooperAction,
    },

    /// #323: set one of a looper's persisted parameters. `Mix`/`Decay` are
    /// clamped to 0..=1.
    SetChainLooperParam {
        chain: ChainId,
        looper: u64,
        param: LooperParam,
    },

    /// #323: choose which of the chain's bound input endpoints a looper records
    /// its dry signal from. `None` ⇒ the chain's first input.
    SetChainLooperInput {
        chain: ChainId,
        looper: u64,
        input: Option<EndpointRef>,
    },

    /// #323: choose which of the chain's bound output endpoints a looper plays
    /// back to. `None` ⇒ the chain's main output.
    SetChainLooperOutput {
        chain: ChainId,
        looper: u64,
        output: Option<EndpointRef>,
    },

    /// #826: reshape a recorded loop — trim its bounds, crop to a region, or
    /// cut a region out. Frame indices over the loop as it stands. The audio
    /// work happens on the control thread behind the runtime door, and is
    /// refused unless the looper is stopped.
    EditChainLooperAudio {
        chain: ChainId,
        looper: u64,
        edit: LoopEdit,
    },

    /// #826: step back one waveform edit. Independent of the transport's
    /// `Undo`, which is a no-op for the single-take looper.
    UndoChainLooperEdit { chain: ChainId, looper: u64 },

    /// #826: step forward one undone waveform edit.
    RedoChainLooperEdit { chain: ChainId, looper: u64 },

    /// #323: remember (or forget) the file holding a looper's recorded audio.
    /// Dispatched by whoever wrote the wav — only the pointer lives in the
    /// project.
    SetChainLooperAudioFile {
        chain: ChainId,
        looper: u64,
        file: Option<String>,
    },

    /// #323 phase 2: link a looper to the preset whose effects it plays through.
    /// The loop records DRY; this id says WHICH preset renders it, so switching
    /// the chain's live preset to solo does not change the loop's tone. Set to
    /// the chain's active preset on RECORD and reassignable via the drawer's
    /// picker. `None` ⇒ the chain's current preset.
    SetChainLooperPreset {
        chain: ChainId,
        looper: u64,
        preset: Option<String>,
    },
}
