//! Unit tests for `from_package` audit-baseline selection. Issue #514.

use super::select_audit_db;

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
