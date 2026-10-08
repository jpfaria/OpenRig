//! Responsibility: finds the stretch of a take that loops on whole passes of its pattern.
//!
//! A loop base is a pattern played several times, and a loop only sounds right
//! when its end runs into its start. FIT therefore keeps whole passes of that
//! pattern from the first attack: the count-in before it goes, and so does an
//! unfinished pass at the end. The region runs [`SEAM_FRAMES`] past the last
//! whole pass, so the wrap seam blends the loop's head with the playing that
//! really followed it — the start of the next pass.

use feature_dsp::repeat_period::repeat_period;

use crate::loop_edit::{content_bounds, SEAM_FRAMES};

/// The last pass still counts when no more than this fraction of it is
/// missing: its final notes may fade below the take's threshold before it ends.
const PASS_TOLERANCE: f64 = 0.25;

/// The region FIT keeps of an interleaved-stereo take recorded at
/// `sample_rate`, in frames — `None` when the take holds no repeat to loop on.
pub fn fit_region(pcm: &[f32], sample_rate: u32) -> Option<(usize, usize)> {
    let frames = pcm.len() / 2;
    let (start, end) = content_bounds(pcm)?;
    let mono: Vec<f32> = pcm[start * 2..end * 2]
        .chunks_exact(2)
        .map(|f| 0.5 * (f[0] + f[1]))
        .collect();
    let pass = repeat_period(&mono, sample_rate)?;
    // The passes played, but never more than the take holds.
    let played = ((end - start) as f64 / pass + PASS_TOLERANCE).floor();
    let held = ((frames - start) as f64 / pass).floor();
    let length = (played.min(held).max(1.0) * pass).round() as usize;
    Some((start, (start + length + SEAM_FRAMES).min(frames)))
}

#[cfg(test)]
#[path = "loop_fit_tests.rs"]
mod tests;
