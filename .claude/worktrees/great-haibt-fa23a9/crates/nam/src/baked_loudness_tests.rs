//! Tests for `baked_loudness`. Issue #402.

use super::*;

#[test]
fn extracts_typical_loudness_value() {
    let header = r#"{"version":"0.5.4","metadata":{"loudness":-10.570879936218262,"gain":0.107},"weights":[..."#;
    let v = extract_loudness_from_header(header).expect("loudness present");
    assert!((v - (-10.5708)).abs() < 0.001);
}

#[test]
fn extracts_positive_loudness() {
    let header = r#""loudness":3.5,"#;
    assert!((extract_loudness_from_header(header).unwrap() - 3.5).abs() < 0.001);
}

#[test]
fn returns_none_when_field_absent() {
    let header = r#"{"version":"0.5.4","metadata":{"gain":0.1},"weights":["#;
    assert!(extract_loudness_from_header(header).is_none());
}

#[test]
fn handles_scientific_notation() {
    let header = r#""loudness":-1.057e1,"gain":0.1"#;
    let v = extract_loudness_from_header(header).expect("ok");
    assert!((v - (-10.57)).abs() < 0.001);
}

#[test]
fn ignores_field_inside_other_text() {
    // The exact key must match including the colon, so a partial match
    // on a substring shouldn't leak through.
    let header = r#"{"loudness_target":-10,"weights":["#;
    assert!(extract_loudness_from_header(header).is_none());
}

#[test]
fn read_loudness_returns_none_for_missing_file() {
    assert!(read_loudness_dbfs("/tmp/this_path_must_not_exist_xyz_42.nam").is_none());
}
