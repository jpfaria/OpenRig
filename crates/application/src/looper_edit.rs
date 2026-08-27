//! Responsibility: describes an edit applied to a recorded loop.
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
    /// Find where the playing actually starts and ends and keep THAT — no
    /// region to give, because the point is that the user does not have to
    /// measure it: the count-in and the late release go, the take stays.
    Fit,
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
            // A fit ignores the selection: it finds its own.
            LoopEditKind::Fit => Self::Fit,
        }
    }

    /// The engine operation and the region this edit resolves to.
    pub fn resolve(&self) -> (LoopEditOp, usize, usize) {
        match *self {
            Self::Trim { start, end } | Self::Crop { start, end } => (LoopEditOp::Keep, start, end),
            Self::Cut { start, end } => (LoopEditOp::Cut, start, end),
            // The region is worked out from the audio itself.
            Self::Fit => (LoopEditOp::Fit, 0, 0),
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
    Fit,
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
    /// Whether the loop is sounding right now — the editor's transport shows
    /// play or stop, and the edits are dead while it is true (the store
    /// refuses to reshape a live take).
    pub playing: bool,
    /// Whether the edit history has anything to step through.
    pub can_undo: bool,
    pub can_redo: bool,
}

#[cfg(test)]
#[path = "looper_edit_tests.rs"]
mod tests;

/// Where the loop is, as a ratio of its own length — what the editor draws
/// its playhead at. Clamped, because a cursor read a tick after an edit
/// shortened the loop can sit past its end.
pub fn playhead_ratio(position_frames: usize, len_frames: usize) -> f32 {
    if len_frames == 0 {
        return 0.0;
    }
    (position_frames as f32 / len_frames as f32).clamp(0.0, 1.0)
}

/// What an edit did, as the editor reports it back to the user.
///
/// "Nothing happened" is a real outcome, not an absence of one: FIT on a take
/// that is already tight, or an edit the store refused, must SAY so — a silent
/// button is indistinguishable from a broken one, which is exactly how this
/// was first reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditOutcome {
    /// The loop changed. Nothing to say: the waveform says it.
    Applied,
    /// The edit ran and left the loop as it was (a FIT with nothing to trim).
    NoChange,
    /// The store refused it (not stopped, empty, or too little would be left).
    Refused,
}

impl EditOutcome {
    /// The code the view switches its message on — 0 quiet, 1 no change,
    /// 2 refused. A code, not a sentence: the strings live in the UI catalog,
    /// translated like every other label.
    pub fn code(self) -> i32 {
        match self {
            Self::Applied => 0,
            Self::NoChange => 1,
            Self::Refused => 2,
        }
    }

    /// Read an edit's result: `None` ⇒ the store refused it; otherwise the
    /// length it left behind, compared with the length before.
    pub fn of(before_len: usize, after: Option<usize>) -> Self {
        match after {
            None => Self::Refused,
            Some(len) if len == before_len => Self::NoChange,
            Some(_) => Self::Applied,
        }
    }
}
