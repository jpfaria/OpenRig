//! Hotfix pin (user, screenshot 2):
//!
//! "BLOCK 'rig:input-4:amp:4': INVALID PARAMETER 'output_db' FOR AMP
//!  MODEL 'nam_vox_ac30': VALUE -5.7059765 DOES NOT ALIGN WITH STEP 0.1"
//!
//! The validator rejects float values that fall between schema-defined
//! grid points. A continuous slider (GUI), an MCP-supplied float, or a
//! scene snapshot of the same can produce such a value at any time.
//! Rejecting it makes the runtime refuse to start the chain -- output
//! meter freezes, user has no sound.
//!
//! Contract pin: `ParameterSpec::validate_value` must accept any
//! in-range continuous-slider value. Step misalignment is a UI/display
//! concern, not a constraint. Out-of-range values are still rejected.

use block_core::param::{float_parameter, ParameterUnit};
use domain::value_objects::ParameterValue;

const OUTPUT_DB_PATH: &str = "output_db";
const OUTPUT_DB_LABEL: &str = "Output";
const OUTPUT_DB_MIN: f32 = -24.0;
const OUTPUT_DB_MAX: f32 = 24.0;
const OUTPUT_DB_STEP: f32 = 0.1;
const OFF_GRID_VALUE: f32 = -5.7059765; // verbatim from the user screenshot

#[test]
fn nam_output_db_spec_accepts_off_grid_in_range_value() {
    let spec = float_parameter(
        OUTPUT_DB_PATH,
        OUTPUT_DB_LABEL,
        None,
        Some(0.0),
        OUTPUT_DB_MIN,
        OUTPUT_DB_MAX,
        OUTPUT_DB_STEP,
        ParameterUnit::Decibels,
    );
    let res = spec.validate_value(&ParameterValue::Float(OFF_GRID_VALUE));
    assert!(
        res.is_ok(),
        "float ParameterSpec must tolerate continuous-slider values \
         within range (off-grid is a UI concern, not a constraint). \
         Got error: {:?}",
        res.err()
    );
}

#[test]
fn float_spec_still_rejects_out_of_range_values() {
    let spec = float_parameter(
        OUTPUT_DB_PATH,
        OUTPUT_DB_LABEL,
        None,
        Some(0.0),
        OUTPUT_DB_MIN,
        OUTPUT_DB_MAX,
        OUTPUT_DB_STEP,
        ParameterUnit::Decibels,
    );
    assert!(
        spec.validate_value(&ParameterValue::Float(999.0)).is_err(),
        "above-range values still rejected"
    );
    assert!(
        spec.validate_value(&ParameterValue::Float(-999.0)).is_err(),
        "below-range values still rejected"
    );
}

#[test]
fn float_spec_accepts_on_grid_value() {
    let spec = float_parameter(
        OUTPUT_DB_PATH,
        OUTPUT_DB_LABEL,
        None,
        Some(0.0),
        OUTPUT_DB_MIN,
        OUTPUT_DB_MAX,
        OUTPUT_DB_STEP,
        ParameterUnit::Decibels,
    );
    assert!(spec.validate_value(&ParameterValue::Float(-5.7)).is_ok());
    assert!(spec.validate_value(&ParameterValue::Float(0.0)).is_ok());
    assert!(spec.validate_value(&ParameterValue::Float(24.0)).is_ok());
    assert!(spec.validate_value(&ParameterValue::Float(-24.0)).is_ok());
}

#[test]
fn float_spec_with_integer_step_tolerates_off_grid_value() {
    // Same tolerance for non-fractional steps (e.g. percent sliders).
    let spec = float_parameter(
        "level",
        "Level",
        None,
        Some(50.0),
        0.0,
        100.0,
        1.0,
        ParameterUnit::Percent,
    );
    assert!(spec.validate_value(&ParameterValue::Float(50.5)).is_ok());
}
