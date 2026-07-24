//! GUI selection state — what the user has selected on the Chains screen.
//!
//! Read by MIDI slots (issue #548) to resolve "active chain / active block"
//! at dispatch time; written by the GUI when the user clicks a chain or
//! a block, and exposed to MCP/gRPC via `QueryKind::Selection` so every
//! adapter sees what the user sees (read-side parity, per the project
//! rules in `.claude/skills/openrig-code-quality/SKILL.md`).
//!
//! `active_block` belongs to `active_chain`: leaving a block selected
//! without a chain would be an invariant violation, so `clear_chain`
//! also clears the block.

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SelectionState {
    pub active_chain: Option<String>,
    pub active_block: Option<String>,
    /// Whether the compact-view UI mode is on for the active chain.
    /// Mutated by `Command::SetCompactViewEnabled` (MIDI slot
    /// `toggle_compact_view`); read by the GUI and by `QueryKind::Selection`.
    pub compact_view_enabled: bool,
    /// Snapshot of the tuner button — GUI keeps it in sync. MIDI slot
    /// `toggle_tuner` reads it to dispatch `SetTunerEnabled { !current }`.
    pub tuner_enabled: bool,
    /// Snapshot of the output mute button. MIDI slot
    /// `toggle_output_mute` reads it to dispatch `SetOutputMuted { !current }`.
    pub output_muted: bool,
    /// Snapshot of the spectrum button. MIDI slot
    /// `toggle_spectrum` reads it to dispatch `SetSpectrumEnabled { !current }`.
    pub spectrum_enabled: bool,
    /// #14: snapshot of the metronome power button. MIDI slot
    /// `toggle_metronome` reads it to dispatch `SetMetronomeEnabled { !current }`.
    pub metronome_enabled: bool,
    /// Snapshot of the active chain's `enabled` flag, so MIDI slot
    /// `toggle_active_chain_enabled` knows what to flip without
    /// touching the project from the slot.
    pub active_chain_enabled: bool,
    /// Snapshot of the active block's `enabled` flag, for
    /// `toggle_active_block_enabled`.
    pub active_block_enabled: bool,
    /// Which parameter `block_param_numeric` writes to on CC. The GUI
    /// keeps this in sync when the user clicks a numeric knob on the
    /// active block (the focused knob is the one the MIDI CC drives).
    pub active_block_param_path: Option<String>,
}

impl SelectionState {
    pub fn clear_chain(&mut self) {
        self.active_chain = None;
        self.active_block = None;
    }

    pub fn clear_block(&mut self) {
        self.active_block = None;
    }
}
