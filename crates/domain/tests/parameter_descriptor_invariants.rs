//! Red-first invariant tests for #572 Phase 1 — `ParameterDescriptor`.
//!
//! Spec: `docs/superpowers/specs/2026-05-27-issue-572-mcp-block-plugin-params-design.md`.
//! These tests fail to compile until the new domain type exists; that compile
//! error IS the RED that unlocks the rest of Phase 1.

use domain::ids::ParameterId;
use domain::parameter_descriptor::{ParameterDescriptor, ParameterKind};
use domain::value_objects::ParameterValue;

#[test]
fn parameter_descriptor_number_rejects_min_ge_max() {
    let id = ParameterId("drive".to_string());
    let result = ParameterDescriptor::number(
        id,
        /* min */ 10.0,
        /* max */ 5.0,
        /* step */ 0.1,
        /* default */ ParameterValue::Float(5.0),
    );
    assert!(result.is_err(), "min >= max must be rejected");
}

#[test]
fn parameter_descriptor_number_rejects_non_positive_step() {
    let id = ParameterId("drive".to_string());
    let zero_step = ParameterDescriptor::number(
        id.clone(),
        0.0,
        10.0,
        /* step */ 0.0,
        ParameterValue::Float(5.0),
    );
    assert!(zero_step.is_err(), "step == 0 must be rejected");

    let negative_step = ParameterDescriptor::number(
        id,
        0.0,
        10.0,
        /* step */ -0.1,
        ParameterValue::Float(5.0),
    );
    assert!(negative_step.is_err(), "negative step must be rejected");
}

#[test]
fn parameter_descriptor_number_rejects_default_out_of_range() {
    let id = ParameterId("drive".to_string());
    let below = ParameterDescriptor::number(
        id.clone(),
        0.0,
        10.0,
        0.1,
        /* default */ ParameterValue::Float(-0.5),
    );
    assert!(below.is_err(), "default below min must be rejected");

    let above = ParameterDescriptor::number(
        id,
        0.0,
        10.0,
        0.1,
        /* default */ ParameterValue::Float(10.5),
    );
    assert!(above.is_err(), "default above max must be rejected");
}

#[test]
fn parameter_descriptor_number_rejects_non_numeric_default() {
    let id = ParameterId("drive".to_string());
    let result = ParameterDescriptor::number(
        id,
        0.0,
        10.0,
        0.1,
        /* default */ ParameterValue::Bool(true),
    );
    assert!(
        result.is_err(),
        "Number kind default must be a numeric ParameterValue (Float/Int)"
    );
}

#[test]
fn parameter_descriptor_number_accepts_valid_inputs() {
    let id = ParameterId("drive".to_string());
    let descriptor = ParameterDescriptor::number(
        id.clone(),
        0.0,
        10.0,
        0.1,
        ParameterValue::Float(5.0),
    )
    .expect("valid inputs must construct");
    assert_eq!(descriptor.id, id);
    assert_eq!(descriptor.default, ParameterValue::Float(5.0));
    assert_eq!(
        descriptor.kind,
        ParameterKind::Number {
            min: 0.0,
            max: 10.0,
            step: 0.1
        }
    );
}

#[test]
fn parameter_descriptor_bool_accepts_bool_default() {
    let id = ParameterId("enabled".to_string());
    let descriptor = ParameterDescriptor::bool(id.clone(), ParameterValue::Bool(true))
        .expect("Bool default must be accepted");
    assert_eq!(descriptor.id, id);
    assert_eq!(descriptor.kind, ParameterKind::Bool);
    assert_eq!(descriptor.default, ParameterValue::Bool(true));
}

#[test]
fn parameter_descriptor_bool_rejects_non_bool_default() {
    let id = ParameterId("enabled".to_string());
    let result = ParameterDescriptor::bool(id, ParameterValue::Float(1.0));
    assert!(result.is_err(), "Bool kind default must be a Bool");
}

#[test]
fn parameter_descriptor_number_accepts_int_default_within_range() {
    let id = ParameterId("voicing".to_string());
    let descriptor = ParameterDescriptor::number(
        id,
        0.0,
        10.0,
        1.0,
        /* default */ ParameterValue::Int(5),
    )
    .expect("Int default within range must be accepted");
    assert_eq!(descriptor.default, ParameterValue::Int(5));
}

// ── Text kind ─────────────────────────────────────────────────────────────

#[test]
fn parameter_descriptor_text_accepts_string_default() {
    let id = ParameterId("label".to_string());
    let descriptor =
        ParameterDescriptor::text(id.clone(), ParameterValue::String("clean".to_string()))
            .expect("String default must be accepted");
    assert_eq!(descriptor.id, id);
    assert_eq!(descriptor.kind, ParameterKind::Text);
    assert_eq!(descriptor.default, ParameterValue::String("clean".to_string()));
}

#[test]
fn parameter_descriptor_text_rejects_non_string_default() {
    let id = ParameterId("label".to_string());
    let result = ParameterDescriptor::text(id, ParameterValue::Bool(true));
    assert!(result.is_err(), "Text kind default must be a String");
}

// ── Option kind ───────────────────────────────────────────────────────────

#[test]
fn parameter_descriptor_option_accepts_default_in_values() {
    let id = ParameterId("voicing".to_string());
    let descriptor = ParameterDescriptor::option(
        id.clone(),
        vec!["A".to_string(), "B".to_string(), "C".to_string()],
        ParameterValue::String("B".to_string()),
    )
    .expect("default within values must be accepted");
    assert_eq!(descriptor.id, id);
    assert_eq!(
        descriptor.kind,
        ParameterKind::Option {
            values: vec!["A".to_string(), "B".to_string(), "C".to_string()],
        }
    );
    assert_eq!(descriptor.default, ParameterValue::String("B".to_string()));
}

#[test]
fn parameter_descriptor_option_rejects_empty_values() {
    let id = ParameterId("voicing".to_string());
    let result = ParameterDescriptor::option(id, vec![], ParameterValue::String("A".to_string()));
    assert!(result.is_err(), "Option kind requires at least one value");
}

#[test]
fn parameter_descriptor_option_rejects_default_not_in_values() {
    let id = ParameterId("voicing".to_string());
    let result = ParameterDescriptor::option(
        id,
        vec!["A".to_string(), "B".to_string()],
        ParameterValue::String("Z".to_string()),
    );
    assert!(result.is_err(), "Option default must appear in values list");
}

#[test]
fn parameter_descriptor_option_rejects_non_string_default() {
    let id = ParameterId("voicing".to_string());
    let result = ParameterDescriptor::option(
        id,
        vec!["A".to_string()],
        ParameterValue::Int(0),
    );
    assert!(result.is_err(), "Option default must be a String");
}

// ── File kind ─────────────────────────────────────────────────────────────

#[test]
fn parameter_descriptor_file_accepts_string_path_default() {
    let id = ParameterId("ir_file".to_string());
    let descriptor = ParameterDescriptor::file(
        id.clone(),
        ParameterValue::String("/path/to.wav".to_string()),
    )
    .expect("String path default must be accepted");
    assert_eq!(descriptor.id, id);
    assert_eq!(descriptor.kind, ParameterKind::File);
    assert_eq!(
        descriptor.default,
        ParameterValue::String("/path/to.wav".to_string())
    );
}

#[test]
fn parameter_descriptor_file_accepts_null_default() {
    let id = ParameterId("ir_file".to_string());
    let descriptor = ParameterDescriptor::file(id, ParameterValue::Null)
        .expect("Null default (no preset file) must be accepted");
    assert_eq!(descriptor.default, ParameterValue::Null);
}

#[test]
fn parameter_descriptor_file_rejects_non_path_default() {
    let id = ParameterId("ir_file".to_string());
    let result = ParameterDescriptor::file(id, ParameterValue::Bool(true));
    assert!(
        result.is_err(),
        "File kind default must be a String path or Null"
    );
}
