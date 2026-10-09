use super::*;

#[test]
fn hundred_db_with_a_20k_passband_at_96k_needs_eight_coefficients() {
    // Same order as the reference design (HIIR) for this spec.
    let coefs = design(100.0, 0.25 - 20_000.0 / 96_000.0);
    assert_eq!(coefs.len, 8);
}

#[test]
fn coefficients_are_stable_allpass_poles_in_ascending_order() {
    let coefs = design(100.0, 0.04);
    let used = &coefs.coefs[..coefs.len];
    assert!(used.iter().all(|&c| c > 0.0 && c < 1.0), "{used:?}");
    assert!(used.windows(2).all(|w| w[0] < w[1]), "{used:?}");
}

#[test]
fn a_spec_beyond_the_capacity_is_capped_not_overflowed() {
    let coefs = design(200.0, 0.001);
    assert_eq!(coefs.len, MAX_COEFS);
    assert!(coefs.coefs.iter().all(|c| c.is_finite()));
}
