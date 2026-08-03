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
mod tests {
    use super::*;

    #[test]
    fn head_weight_ramps_from_almost_zero_to_almost_one() {
        // #614's equal-gain ramp: i+1 over xfade+1, so the pair always sums to
        // 1 (no overshoot) and neither end sits exactly at 0 or 1.
        assert_eq!(head_weight(0, 3), 0.25);
        assert_eq!(head_weight(1, 3), 0.5);
        assert_eq!(head_weight(2, 3), 0.75);
    }

    #[test]
    fn the_pair_of_weights_always_sums_to_one() {
        for xfade in 1..16usize {
            for i in 0..xfade {
                let w = head_weight(i, xfade);
                assert!(
                    ((w + (1.0 - w)) - 1.0).abs() < f32::EPSILON,
                    "equal-gain: the seam must never overshoot the source peak"
                );
            }
        }
    }
}
