//! Unit tests for `from_package` audit-baseline selection. Issue #514.

use super::{resolve_output_db, select_audit_db};

#[test]
fn per_capture_audit_takes_precedence_over_top_level() {
    // Manifest says +6 dB top-level, but the matched capture overrides
    // it with -10 dB. The wrapper must use the capture's value.
    assert_eq!(select_audit_db(Some(-10.0), Some(6.0)), Some(-10.0));
}

#[test]
fn top_level_audit_is_used_when_capture_lacks_value() {
    // IR plugins with a uniform baseline (old format) still work.
    assert_eq!(select_audit_db(None, Some(-4.5)), Some(-4.5));
}

#[test]
fn capture_audit_used_when_top_level_missing() {
    // Current shape of IR manifests in OpenRig-plugins — audit lives
    // only on the capture.
    assert_eq!(select_audit_db(Some(2.5), None), Some(2.5));
}

#[test]
fn no_audit_anywhere_returns_none() {
    assert_eq!(select_audit_db(None, None), None);
}

// --- resolve_output_db: user knob is the absolute applied level (#655) ---

#[test]
fn user_output_db_param_overrides_audit_baseline() {
    // The user dragged the Output knob to +3 dB; that is the applied
    // level regardless of the capture's audit baseline.
    assert_eq!(
        resolve_output_db(Some(3.0), Some(-22.9), Some(6.0)),
        Some(3.0)
    );
}

#[test]
fn user_output_db_zero_is_respected_not_treated_as_absent() {
    // 0 dB is a deliberate user choice (absolute), NOT "fall back to
    // audit". Distinguishes Some(0.0) from None.
    assert_eq!(resolve_output_db(Some(0.0), Some(-22.9), None), Some(0.0));
}

#[test]
fn legacy_preset_without_param_falls_back_to_capture_audit() {
    // No output_db in the preset (pre-#655) → keep the per-capture audit
    // loudness. Volume invariant #10: existing presets must not change.
    assert_eq!(resolve_output_db(None, Some(-22.9), Some(6.0)), Some(-22.9));
}

#[test]
fn legacy_preset_falls_back_to_manifest_audit_when_capture_lacks_value() {
    assert_eq!(resolve_output_db(None, None, Some(-4.5)), Some(-4.5));
}

#[test]
fn no_param_and_no_audit_anywhere_returns_none() {
    assert_eq!(resolve_output_db(None, None, None), None);
}
