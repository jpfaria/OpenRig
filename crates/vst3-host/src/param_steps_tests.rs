use super::{is_selector, MAX_SELECTOR_STEPS};

#[test]
fn continuous_parameter_is_not_a_selector() {
    assert!(!is_selector(0));
}

#[test]
fn stepped_parameter_up_to_the_cap_is_a_selector() {
    assert!(is_selector(1));
    assert!(is_selector(MAX_SELECTOR_STEPS));
}

#[test]
fn integer_parameter_with_a_huge_range_is_not_a_selector() {
    // A DPF integer parameter reports one step per value: listing every step
    // froze the app reading the mimo plugins' parameters (#1104).
    assert!(!is_selector(MAX_SELECTOR_STEPS + 1));
    assert!(!is_selector(i32::MAX));
}
