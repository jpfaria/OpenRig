
use super::*;
#[test]
fn six_native_overrides_present() {
    for id in [
        "analog_warm",
        "digital_clean",
        "modulated_delay",
        "reverse",
        "slapback",
        "tape_vintage",
    ] {
        assert!(
            model_color_override(id).is_some(),
            "missing override for {id}"
        );
    }
}
#[test]
fn unknown_model_returns_none() {
    assert!(model_color_override("invalid_delay").is_none());
}
