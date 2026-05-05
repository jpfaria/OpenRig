
use super::*;
#[test]
fn known_models_return_some() {
    assert!(model_color_override("american_2x12").is_some());
    assert!(model_color_override("brit_4x12").is_some());
    assert!(model_color_override("vintage_1x12").is_some());
}
#[test]
fn unknown_model_returns_none() {
    assert!(model_color_override("g12t_75_4x12").is_none());
}
