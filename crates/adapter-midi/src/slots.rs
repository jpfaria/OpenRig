//! Slot dispatch (issue #548 Phase 3c).
//!
//! Pure function that maps `(slot_name, MIDI message, SelectionState) →
//! Command`. The MIDI daemon (Phase 4) calls this for each profile
//! binding that matched the incoming message, then forwards the
//! returned Command to `LocalDispatcher`.
//!
//! Pure on purpose: trivially testable without a dispatcher mock, and
//! no I/O on the dispatch path. CC continuous slots (`chain_volume`,
//! `block_param_numeric`) need parameter-schema lookup to scale 0-127
//! → range; they are handled in a follow-up sub-phase.

use application::command::{ChainId, Command, RigNavKind};
use application::SelectionState;

/// MIDI message in the shape the daemon resolves before calling slots:
/// already normalised (channel + value), no port name (the profile's
/// `source` filter already matched). Mirrors the four MIDI 1.0 channel
/// voice messages this adapter supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncomingMessage {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    ProgramChange { channel: u8, program: u8 },
}

impl IncomingMessage {
    /// The "value byte" that wildcard slots (`jump_preset_n`, etc.) read
    /// to parameterise their action. PC -> `program`, CC -> `value`,
    /// Note On -> `velocity`, Note Off -> `0`.
    pub fn value_byte(&self) -> u8 {
        match self {
            Self::ProgramChange { program, .. } => *program,
            Self::ControlChange { value, .. } => *value,
            Self::NoteOn { velocity, .. } => *velocity,
            Self::NoteOff { .. } => 0,
        }
    }
}

/// Build the `Command` for a catalog slot. Returns `None` when the slot
/// name is unknown (defensive — the YAML parser already validates names
/// at load time, so this should never trigger in production).
///
/// Slots that need the active chain id and there is none active return
/// `None` (the daemon simply doesn't dispatch — a footswitch press on a
/// not-yet-loaded project does nothing, as expected).
pub fn slot_to_command(
    slot: &str,
    msg: &IncomingMessage,
    selection: &SelectionState,
) -> Option<Command> {
    let active_chain = || selection.active_chain.clone().map(ChainId);
    match slot {
        // --- Chain navigation ---
        "prev_chain" => Some(Command::SelectActiveChainRelative { delta: -1 }),
        "next_chain" => Some(Command::SelectActiveChainRelative { delta: 1 }),

        // --- Block navigation ---
        "prev_block_1" => Some(Command::SelectActiveBlockRelative { delta: -1 }),
        "next_block_1" => Some(Command::SelectActiveBlockRelative { delta: 1 }),
        "prev_block_2" => Some(Command::SelectActiveBlockRelative { delta: -2 }),
        "next_block_2" => Some(Command::SelectActiveBlockRelative { delta: 2 }),

        // --- Rig nav (preset / scene, on the active chain) ---
        "prev_preset" => Some(Command::ApplyRigNav {
            chain: active_chain()?,
            kind: RigNavKind::StepPreset(-1),
        }),
        "next_preset" => Some(Command::ApplyRigNav {
            chain: active_chain()?,
            kind: RigNavKind::StepPreset(1),
        }),
        "prev_scene" => Some(Command::ApplyRigNav {
            chain: active_chain()?,
            kind: RigNavKind::StepScene(-1),
        }),
        "next_scene" => Some(Command::ApplyRigNav {
            chain: active_chain()?,
            kind: RigNavKind::StepScene(1),
        }),
        "jump_preset_n" => Some(Command::ApplyRigNav {
            chain: active_chain()?,
            kind: RigNavKind::Preset(msg.value_byte() as i32),
        }),
        "jump_scene_n" => Some(Command::ApplyRigNav {
            chain: active_chain()?,
            kind: RigNavKind::Scene(msg.value_byte() as i32),
        }),

        // --- View toggle ---
        "toggle_compact_view" => Some(Command::SetCompactViewEnabled {
            enabled: !selection.compact_view_enabled,
        }),

        // --- Other toggles & continuous slots: handled in a follow-up
        //     phase (read-state-then-toggle for tuner/mute/spectrum;
        //     CC range scaling for chain_volume / block_param_numeric).
        _ => None,
    }
}
