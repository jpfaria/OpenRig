
use super::*;
#[test]
fn known_models_present() {
    assert!(model_color_override("compressor_studio_clean").is_some());
    assert!(model_color_override("gate_basic").is_some());
}
#[test]
fn unknown_model_returns_none() {
    assert!(model_color_override("limiter_brickwall").is_none());
}
