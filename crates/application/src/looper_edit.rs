//! #826 — reshaping a recorded loop: trim / crop / cut, on the control thread.
//!
//! Pure transforms over an interleaved-stereo buffer: frame indices in, a new
//! buffer out. No engine state, no I/O — the caller reads the loop with
//! `LooperSlot::export_raw` and installs the result with
//! `LooperSlot::load_layer`.

use engine::crossfade::head_weight;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Frames blended at a seam. ~1.3 ms at 48 kHz: long enough to kill the step,
/// short enough that nothing musical is smeared.
pub const SEAM_FRAMES: usize = 64;

/// Shortest loop an edit may leave behind — two seams' worth, so a seam always
/// has room and a stray drag cannot reduce a take to a click.
pub const MIN_LOOP_FRAMES: usize = SEAM_FRAMES * 2;

/// One reshaping of a recorded loop. Frame indices over the loop, `end`
/// exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum LoopEdit {
    /// Move the loop bounds inward: keep `[start, end)`.
    Trim { start: usize, end: usize },
    /// Keep `[start, end)` and discard the rest. The same audio transform as
    /// `Trim`; a distinct variant so the user's intent survives into MCP.
    Crop { start: usize, end: usize },
    /// Drop `[start, end)` and join the two halves.
    Cut { start: usize, end: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopEditError {
    /// A bound lies outside the loop.
    OutOfRange,
    /// `start >= end`.
    EmptyRegion,
    /// The result would be shorter than [`MIN_LOOP_FRAMES`].
    ResultTooShort,
}

impl std::fmt::Display for LoopEditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfRange => write!(f, "the edit region is outside the loop"),
            Self::EmptyRegion => write!(f, "the edit region is empty"),
            Self::ResultTooShort => write!(f, "the edit would leave too little audio"),
        }
    }
}

impl std::error::Error for LoopEditError {}

impl LoopEdit {
    fn bounds(&self) -> (usize, usize) {
        match *self {
            Self::Trim { start, end } | Self::Crop { start, end } | Self::Cut { start, end } => {
                (start, end)
            }
        }
    }
}

/// Apply `edit` to an interleaved-stereo loop, returning the new loop.
///
/// Every result is seam-blended so playback wraps (and a cut joins) without a
/// step: the last [`SEAM_FRAMES`] are folded into the head with the equal-gain
/// overlap-add of #614, so the result is `SEAM_FRAMES` shorter than the naive
/// selection.
pub fn apply_edit(pcm: &[f32], edit: LoopEdit) -> Result<Vec<f32>, LoopEditError> {
    let len = pcm.len() / 2;
    let (start, end) = edit.bounds();
    if start >= end {
        return Err(LoopEditError::EmptyRegion);
    }
    if end > len {
        return Err(LoopEditError::OutOfRange);
    }

    let kept: Vec<f32> = match edit {
        LoopEdit::Trim { .. } | LoopEdit::Crop { .. } => {
            if (end - start) < MIN_LOOP_FRAMES + SEAM_FRAMES {
                return Err(LoopEditError::ResultTooShort);
            }
            pcm[start * 2..end * 2].to_vec()
        }
        LoopEdit::Cut { .. } => {
            let head = &pcm[..start * 2];
            let tail = &pcm[end * 2..];
            // Both halves must be able to spare the join's overlap, and what
            // is left must still be a loop.
            if head.len() / 2 <= SEAM_FRAMES
                || tail.len() / 2 <= SEAM_FRAMES
                || (head.len() + tail.len()) / 2 < MIN_LOOP_FRAMES + SEAM_FRAMES * 2
            {
                return Err(LoopEditError::ResultTooShort);
            }
            join_blend(head, tail, SEAM_FRAMES)
        }
    };

    Ok(seam_blend(&kept, SEAM_FRAMES))
}

/// Overlap-add `tail` onto the end of `head`, returning a buffer `xfade`
/// frames shorter than the two together: the join fades one into the other
/// instead of butt-splicing them, so a cut never steps.
fn join_blend(head: &[f32], tail: &[f32], xfade: usize) -> Vec<f32> {
    let h = head.len() / 2;
    let keep = h - xfade;
    let mut out = Vec::with_capacity(head.len() + tail.len() - xfade * 2);
    out.extend_from_slice(&head[..keep * 2]);
    for i in 0..xfade {
        // The tail fades IN over the head's dropped frames, which fade out.
        let w = head_weight(i, xfade);
        for ch in 0..2 {
            out.push(tail[i * 2 + ch] * w + head[(keep + i) * 2 + ch] * (1.0 - w));
        }
    }
    out.extend_from_slice(&tail[xfade * 2..]);
    out
}

/// Overlap-add the last `xfade` frames into the head, returning a buffer
/// `xfade` frames shorter. The new first frame is ~the source frame that
/// followed the new last frame, so the loop wraps continuously (#614).
fn seam_blend(pcm: &[f32], xfade: usize) -> Vec<f32> {
    let n = pcm.len() / 2;
    if xfade == 0 || n < xfade * 2 + 1 {
        return pcm.to_vec();
    }
    let m = n - xfade;
    let mut out = Vec::with_capacity(m * 2);
    for i in 0..xfade {
        let w = head_weight(i, xfade);
        for ch in 0..2 {
            out.push(pcm[i * 2 + ch] * w + pcm[(m + i) * 2 + ch] * (1.0 - w));
        }
    }
    out.extend_from_slice(&pcm[xfade * 2..m * 2]);
    out
}

/// Bucketed peak envelope for drawing: `buckets` values in 0..=1, each the
/// loudest absolute sample (either channel) in that slice of the loop. A loop
/// with fewer frames than buckets still fills every bucket.
pub fn peaks(pcm: &[f32], buckets: usize) -> Vec<f32> {
    if buckets == 0 {
        return Vec::new();
    }
    let frames = pcm.len() / 2;
    if frames == 0 {
        return vec![0.0; buckets];
    }
    (0..buckets)
        .map(|b| {
            let from = (b * frames / buckets).min(frames - 1);
            let to = (((b + 1) * frames) / buckets).max(from + 1).min(frames);
            pcm[from * 2..to * 2]
                .iter()
                .fold(0.0f32, |a, s| a.max(s.abs()))
                .min(1.0)
        })
        .collect()
}

#[cfg(test)]
#[path = "looper_edit_tests.rs"]
mod tests;
