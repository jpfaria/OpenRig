//! #826 — the command-facing shape of a loop edit.
//!
//! The audio itself lives in `engine::loop_edit` (pure, serde-free, reachable
//! from the audio-side crates). This is the transport-facing half: the enum a
//! GUI button, an MCP tool or a gRPC client fills in, plus the resolution to
//! the operation the engine performs.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use engine::loop_edit::{LoopEditError, LoopEditOp, MIN_LOOP_FRAMES, SEAM_FRAMES};

/// One reshaping of a recorded loop. Frame indices over the loop, `end`
/// exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum LoopEdit {
    /// Move the loop bounds inward: keep `[start, end)`.
    Trim { start: usize, end: usize },
    /// Keep `[start, end)` and discard the rest. The same audio transform as
    /// `Trim`; a distinct variant so the user's intent survives into MCP and
    /// into the log.
    Crop { start: usize, end: usize },
    /// Drop `[start, end)` and join the two halves.
    Cut { start: usize, end: usize },
}

impl LoopEdit {
    /// Build the edit a selection describes, from the two ratios of the loop a
    /// view hands back (0..=1) and the loop's current length.
    ///
    /// The rule lives HERE, not in a frontend: a selection is clamped to the
    /// loop and ordered, so a handle dragged past the edge means "all the way"
    /// and a backwards drag still describes the same region. A GUI that worked
    /// this out itself would be a second, divergent answer to what a selection
    /// means — and MCP would have none at all.
    pub fn from_ratios(kind: LoopEditKind, len_frames: usize, from: f32, to: f32) -> Self {
        let at = |v: f32| (v.clamp(0.0, 1.0) * len_frames as f32).round() as usize;
        let (a, b) = (at(from).min(len_frames), at(to).min(len_frames));
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        match kind {
            LoopEditKind::Trim => Self::Trim { start, end },
            LoopEditKind::Crop => Self::Crop { start, end },
            LoopEditKind::Cut => Self::Cut { start, end },
        }
    }

    /// The engine operation and the region this edit resolves to.
    pub fn resolve(&self) -> (LoopEditOp, usize, usize) {
        match *self {
            Self::Trim { start, end } | Self::Crop { start, end } => (LoopEditOp::Keep, start, end),
            Self::Cut { start, end } => (LoopEditOp::Cut, start, end),
        }
    }
}

/// Which reshaping a selection asks for. A frontend picks one; the region it
/// means is resolved by [`LoopEdit::from_ratios`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopEditKind {
    Trim,
    Crop,
    Cut,
}

/// #826: one loop as the waveform editor reads it — a FINISHED reading, so no
/// samples cross the frontend seam (#127).
#[derive(Debug, Clone, PartialEq)]
pub struct LoopEditReading {
    /// Peak envelope, 0..=1, one value per bar the view draws.
    pub peaks: Vec<f32>,
    /// The loop's current length: what the selection's ratios resolve against.
    pub len_frames: usize,
    /// The same length as the clock the panel shows ("0:08"), formatted by the
    /// frontend that knows the rate — the editor states how much audio it is
    /// about to reshape instead of making the user guess from a drawing.
    pub length_label: String,
    /// Whether the edit history has anything to step through.
    pub can_undo: bool,
    pub can_redo: bool,
}

#[cfg(test)]
#[path = "looper_edit_tests.rs"]
mod tests;
