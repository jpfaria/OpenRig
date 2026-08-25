//! Tests for the shared seam-crossfade curve (#826).

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
