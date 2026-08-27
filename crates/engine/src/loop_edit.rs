//! Responsibility: reshapes a recorded loop.
//! #826 — reshaping a recorded loop: trim / crop / cut, on the control thread.
//!
//! Pure transforms over an interleaved-stereo buffer: frame indices in, a new
//! buffer out. No engine state, no I/O — the caller reads the loop with
//! `LooperSlot::export_raw` and installs the result with
//! `LooperSlot::load_layer`.

use crate::crossfade::head_weight;

/// Frames blended at a seam. ~1.3 ms at 48 kHz: long enough to kill the step,
/// short enough that nothing musical is smeared.
pub const SEAM_FRAMES: usize = 64;

/// Shortest loop an edit may leave behind — two seams' worth, so a seam always
/// has room and a stray drag cannot reduce a take to a click.
pub const MIN_LOOP_FRAMES: usize = SEAM_FRAMES * 2;

/// What a reshaping does to the region it is handed. The command layer owns
/// the serializable, MCP-facing shape (`application::looper_edit::LoopEdit`);
/// this is the audio operation it resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopEditOp {
    /// Keep the region: the loop bounds move inward. `Trim` and `Crop` differ
    /// in intent, not in audio, so they resolve to the same operation.
    Keep,
    /// Drop the region and join the two halves.
    Cut,
    /// Find where the playing actually starts and ends and keep THAT — the
    /// region argument is ignored. The one-button answer to "make the loop
    /// right": a take carries a count-in at the head and a late release at the
    /// tail, and neither belongs in a loop that repeats.
    Fit,
}

/// The absolute floor: anything quieter is digital silence, whatever else the
/// take contains. -60 dBFS.
const SILENCE_FLOOR: f32 = 0.001;

/// How far below the take's OWN peak the playing is assumed to stay above.
/// -32 dB: a real take's "silence" is hiss, hum and string noise — a live rig
/// idles around -45 dBFS, far above the absolute floor — so the threshold has
/// to be relative to what was actually played, or FIT finds nothing to trim
/// and the button looks dead. A note decaying more than 32 dB below the take's
/// loudest moment is a tail, not a phrase.
const CONTENT_PEAK_RATIO: f32 = 0.025;

/// How long the signal must stay above the floor before it counts as playing
/// (and stay below it before it counts as over). ~1 ms at 48 kHz: long enough
/// that one stray sample of noise is not "the take", short enough that no
/// attack is clipped.
const CONTENT_RUN_FRAMES: usize = 48;

/// A little air kept on each side of the music, so `Fit` never eats the very
/// start of an attack or the tail of a ring-out.
const CONTENT_MARGIN_FRAMES: usize = 16;

/// Where the playing actually starts and ends, in frames — `None` when the
/// take is silence all the way through.
///
/// A run-length rule, not a single-sample one: a lone tick of noise is not the
/// take starting, and a single zero-crossing is not it ending. The bounds are
/// widened by [`CONTENT_MARGIN_FRAMES`] so an attack is never clipped.
pub fn content_bounds(pcm: &[f32]) -> Option<(usize, usize)> {
    let frames = pcm.len() / 2;
    if frames == 0 {
        return None;
    }
    // The threshold rides the take's own peak, never below the absolute floor.
    let peak = pcm.iter().fold(0.0f32, |a, s| a.max(s.abs()));
    let threshold = (peak * CONTENT_PEAK_RATIO).max(SILENCE_FLOOR);
    let loud = |f: usize| pcm[f * 2].abs().max(pcm[f * 2 + 1].abs()) > threshold;

    let mut run = 0;
    let mut start = None;
    for f in 0..frames {
        if loud(f) {
            run += 1;
            if run >= CONTENT_RUN_FRAMES {
                start = Some(f + 1 - run);
                break;
            }
        } else {
            run = 0;
        }
    }
    let start = start?;

    let mut run = 0;
    let mut end = frames;
    for f in (start..frames).rev() {
        if loud(f) {
            run += 1;
            if run >= CONTENT_RUN_FRAMES {
                end = f + run;
                break;
            }
        } else {
            run = 0;
        }
    }

    Some((
        start.saturating_sub(CONTENT_MARGIN_FRAMES),
        (end + CONTENT_MARGIN_FRAMES).min(frames),
    ))
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

/// Apply `edit` to an interleaved-stereo loop, returning the new loop.
///
/// Every result is seam-blended so playback wraps (and a cut joins) without a
/// step: the last [`SEAM_FRAMES`] are folded into the head with the equal-gain
/// overlap-add of #614, so the result is `SEAM_FRAMES` shorter than the naive
/// selection.
pub fn apply_edit(
    pcm: &[f32],
    op: LoopEditOp,
    start: usize,
    end: usize,
) -> Result<Vec<f32>, LoopEditError> {
    let len = pcm.len() / 2;
    // `Fit` works out its own region: the caller has no way to know where the
    // music is, which is the whole point of the button.
    let (start, end) = if matches!(op, LoopEditOp::Fit) {
        content_bounds(pcm).ok_or(LoopEditError::EmptyRegion)?
    } else {
        (start, end)
    };
    if start >= end {
        return Err(LoopEditError::EmptyRegion);
    }
    if end > len {
        return Err(LoopEditError::OutOfRange);
    }

    let kept: Vec<f32> = match op {
        LoopEditOp::Keep | LoopEditOp::Fit => {
            if (end - start) < MIN_LOOP_FRAMES + SEAM_FRAMES {
                return Err(LoopEditError::ResultTooShort);
            }
            pcm[start * 2..end * 2].to_vec()
        }
        LoopEditOp::Cut => {
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
#[path = "loop_edit_tests.rs"]
mod tests;
