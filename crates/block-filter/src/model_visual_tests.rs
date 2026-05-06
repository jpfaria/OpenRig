use super::*;
#[test]
fn eq_three_band_basic_pinned() {
    assert!(model_color_override("eq_three_band_basic").is_some());
}
#[test]
fn unknown_model_returns_none() {
    assert!(model_color_override("eq_eight_band").is_none());
}
