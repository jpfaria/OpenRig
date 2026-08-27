//! Responsibility: defines the curve a seam crossfades with.
//! The seam crossfade curve, defined once (#614's shape, shared since #826).
//!
//! Equal-GAIN, not equal-power: the two weights sum to 1, so an overlap-add
//! seam can never overshoot the source peak — on a high-gain chain an
//! overshoot is an audible click that sounds like clipping every time the loop
//! wraps.

/// Weight of the fading-IN frame at overlap position `i` of `xfade` frames.
/// The fading-OUT partner takes `1.0 - head_weight(i, xfade)`.
#[inline]
pub fn head_weight(i: usize, xfade: usize) -> f32 {
    (i + 1) as f32 / (xfade + 1) as f32
}

#[cfg(test)]
#[path = "crossfade_tests.rs"]
mod tests;
