//! Red-first tests pinning the new "0 dB on the UI is unity, unless
//! the manifest audit explicitly opts in" contract.
//!
//! Background: with `audit_overrides_baked_output == true` (i.e. the
//! model was loaded via `from_package`, which is the only path the
//! GUI uses today), the trainer-baked `recommended_*_db` values must
//! NOT be applied on top of the user knobs. The audit pipeline has
//! already pre-computed any offset into `params.{output_level_db}`,
//! so the user's `0 / 0` means "unity, end-to-end". This was already
//! true for the output side; these tests assert the same shape for
//! the input side and pin the legacy fallback path so audited models
//! never see the trainer values double-counted.

use super::{resolve_gain_offsets, GainOffsetInputs};

#[test]
fn audit_override_ignores_recommended_input_and_output() {
    // The model was audited: the trainer recommendations must not
    // stack on top of the user knobs. UI shows 0 / 0 → real gain is
    // 0 / 0 (unity).
    let (in_db, out_db) = resolve_gain_offsets(GainOffsetInputs {
        input_level_db: 0.0,
        output_level_db: 0.0,
        recommended_input_db: -3.0,
        recommended_output_db: -8.0,
        audit_overrides_baked_output: true,
    });
    assert_eq!(in_db, 0.0, "audit override: trainer input rec ignored");
    assert_eq!(out_db, 0.0, "audit override: trainer output rec ignored");
}

#[test]
fn audit_override_passes_user_knobs_through_untouched() {
    // User dialled +6 / +3 — must reach the processor as +6 / +3,
    // not as +3 / -5 once the trainer rec is added on top.
    let (in_db, out_db) = resolve_gain_offsets(GainOffsetInputs {
        input_level_db: 6.0,
        output_level_db: 3.0,
        recommended_input_db: -3.0,
        recommended_output_db: -8.0,
        audit_overrides_baked_output: true,
    });
    assert_eq!(in_db, 6.0);
    assert_eq!(out_db, 3.0);
}

#[test]
fn legacy_no_audit_still_applies_trainer_recommendations() {
    // Backward-compat path: when the manifest has no audit field, the
    // trainer baked values still take effect — otherwise un-audited
    // models would suddenly play 7-8 dB louder than before.
    let (in_db, out_db) = resolve_gain_offsets(GainOffsetInputs {
        input_level_db: 0.0,
        output_level_db: 0.0,
        recommended_input_db: -3.0,
        recommended_output_db: -8.0,
        audit_overrides_baked_output: false,
    });
    assert_eq!(in_db, -3.0, "legacy: trainer input rec applies");
    assert_eq!(out_db, -8.0, "legacy: trainer output rec applies");
}

#[test]
fn legacy_no_audit_sums_user_knobs_with_recommendations() {
    let (in_db, out_db) = resolve_gain_offsets(GainOffsetInputs {
        input_level_db: 2.0,
        output_level_db: 4.0,
        recommended_input_db: -3.0,
        recommended_output_db: -8.0,
        audit_overrides_baked_output: false,
    });
    assert_eq!(in_db, -1.0);
    assert_eq!(out_db, -4.0);
}

#[test]
fn zero_recommendations_are_unity_in_either_branch() {
    for audit in [true, false] {
        let (in_db, out_db) = resolve_gain_offsets(GainOffsetInputs {
            input_level_db: 0.0,
            output_level_db: 0.0,
            recommended_input_db: 0.0,
            recommended_output_db: 0.0,
            audit_overrides_baked_output: audit,
        });
        assert_eq!(in_db, 0.0, "audit={audit}");
        assert_eq!(out_db, 0.0, "audit={audit}");
    }
}
