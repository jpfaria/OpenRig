use super::*;

#[test]
fn a_reading_inside_the_tolerance_is_in_tune() {
    assert!(in_tune(0.0));
    assert!(in_tune(IN_TUNE_CENTS - 0.01));
    assert!(in_tune(-(IN_TUNE_CENTS - 0.01)));
}

#[test]
fn a_reading_outside_the_tolerance_is_not() {
    assert!(!in_tune(IN_TUNE_CENTS + 0.01));
    assert!(!in_tune(-(IN_TUNE_CENTS + 0.01)));
    assert!(!in_tune(-7.0), "the owner's -7 ct is still out of tune");
}

#[test]
fn the_tolerance_is_tight_enough_to_hear_no_beating() {
    assert!(IN_TUNE_CENTS <= 3.0);
}
