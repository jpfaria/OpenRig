use super::*;

#[test]
fn midpoint_is_unity() {
    assert_eq!(unity_knob_db(50.0, -24.0, 16.0), 0.0);
    assert_eq!(output_knob_db(50.0), 0.0);
}

#[test]
fn ends_reach_their_own_limits() {
    assert_eq!(unity_knob_db(0.0, -24.0, 16.0), -24.0);
    assert_eq!(unity_knob_db(100.0, -24.0, 16.0), 16.0);
    assert_eq!(output_knob_db(0.0), OUTPUT_KNOB_MIN_DB);
    assert_eq!(output_knob_db(100.0), OUTPUT_KNOB_MAX_DB);
}

#[test]
fn each_half_is_linear_in_db() {
    assert!((unity_knob_db(25.0, -24.0, 16.0) + 12.0).abs() < 1e-6);
    assert!((unity_knob_db(75.0, -24.0, 16.0) - 8.0).abs() < 1e-6);
}

#[test]
fn rises_monotonically_and_clamps() {
    let mut last = f32::NEG_INFINITY;
    for step in 0..=100 {
        let db = output_knob_db(step as f32);
        assert!(db >= last, "not monotonic at {step} %");
        last = db;
    }
    assert_eq!(output_knob_db(-10.0), OUTPUT_KNOB_MIN_DB);
    assert_eq!(output_knob_db(150.0), OUTPUT_KNOB_MAX_DB);
}
