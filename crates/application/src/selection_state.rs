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
