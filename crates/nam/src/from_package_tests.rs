//! Tests for `from_package`.
//!
//! Issue #630: a NAM grid pedal (knobs → nearest `.nam` capture) always
//! selects the nearest capture, regardless of knob position. The legacy
//! #402 "knob at 0 == passthrough" rule has been REMOVED — a capture at
//! drive=0 is a real capture, not "off". On/off is the engine enable toggle.

use super::*;
use domain::value_objects::ParameterValue as BlockParameterValue;
use plugin_loader::manifest::{GridCapture, GridParameter, ParameterValue};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn drive_grid_parameters() -> Vec<GridParameter> {
    vec![GridParameter {
        name: "drive".to_string(),
        display_name: None,
        values: vec![ParameterValue::Number(0.0), ParameterValue::Number(5.0)],
    }]
}

fn drive_captures() -> Vec<GridCapture> {
    fn cell(drive: f64, file: &str) -> GridCapture {
        let mut values = BTreeMap::new();
        values.insert("drive".to_string(), ParameterValue::Number(drive));
        GridCapture {
            values,
            file: PathBuf::from(file),
            output_gain_db: None,
        }
    }
    vec![cell(0.0, "captures/drive_min.nam"), cell(5.0, "captures/drive_hi.nam")]
}

/// Issue #630: a grid pedal at the axis minimum (drive=0) must resolve to the
/// REAL drive=0 capture — not bail out to passthrough / no model. The OLD
/// `any_zero_knob` rule made `build_from_package` short-circuit before
/// `resolve_capture` ever ran, silently unloading the model. With the rule
/// removed, drive=0 selects the drive=0 capture's file.
#[test]
fn grid_knob_at_zero_selects_the_zero_capture_not_passthrough() {
    let parameters = drive_grid_parameters();
    let captures = drive_captures();
    let mut ps = ParameterSet::default();
    ps.insert("drive", BlockParameterValue::Float(0.0));

    let resolved = plugin_loader::dispatch::resolve_capture(&parameters, &captures, &ps)
        .expect("drive=0 must resolve to a real capture, not None/passthrough");
    assert_eq!(
        resolved.file,
        PathBuf::from("captures/drive_min.nam"),
        "issue #630: drive=0 must select the drive=0 capture; 0 is a real \
         capture, not off"
    );
}

/// Sanity: a non-zero knob still selects the matching high capture.
#[test]
fn grid_knob_above_zero_selects_the_high_capture() {
    let parameters = drive_grid_parameters();
    let captures = drive_captures();
    let mut ps = ParameterSet::default();
    ps.insert("drive", BlockParameterValue::Float(5.0));

    let resolved = plugin_loader::dispatch::resolve_capture(&parameters, &captures, &ps)
        .expect("drive=5 must resolve to a capture");
    assert_eq!(resolved.file, PathBuf::from("captures/drive_hi.nam"));
}

/// New contract: the audit's `manifest.output_gain_db` is NOT added
/// silently to `params.output_level_db` at load time. Whoever creates
/// or loads the block is expected to put the audit value directly
/// into the user-visible param. The UI knob `output_db` then reads
/// what the engine actually applies — no hidden offset.
#[test]
fn manifest_output_gain_db_does_not_stack_onto_user_param() {
    // Mimic the previous behaviour: user set output_db = 0, manifest
    // ships audit = -10 dB. Old code returned -10 (audit auto-stacks).
    // New code returns 0 (audit lives in the preset, not in the
    // loader).
    let resolved = crate::from_package::resolve_user_output_level_db(0.0, Some(-10.0));
    assert_eq!(
        resolved, 0.0,
        "audit must not be summed at load time; the preset carries it"
    );
}

#[test]
fn user_output_param_is_passed_through_when_no_audit_in_manifest() {
    let resolved = crate::from_package::resolve_user_output_level_db(2.5, None);
    assert_eq!(resolved, 2.5);
}

#[test]
fn user_output_param_is_passed_through_even_when_audit_is_present() {
    let resolved = crate::from_package::resolve_user_output_level_db(-3.0, Some(-10.0));
    assert_eq!(resolved, -3.0);
}
