//! Red-first invariant tests for #572 Phase 1 — `ParameterDescriptor`.
//!
//! Spec: `docs/superpowers/specs/2026-05-27-issue-572-mcp-block-plugin-params-design.md`.
//! These tests fail to compile until the new domain type exists; that compile
//! error IS the RED that unlocks the rest of Phase 1.

use domain::ids::ParameterId;
use domain::parameter_descriptor::ParameterDescriptor;
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
