//! Tests for the Tone Doctor GUI mapping (pure function).
//!
//! The diagnosis itself is the dispatcher's (`application`'s tests pin it);
//! what the panel owns is turning that verdict into fields a human reads.

use application::tone_doctor_report::{ToneFix, ToneMeasurement, ToneReport};
use domain::ids::BlockId;

use super::*;

fn measurement(value: f32, limit: f32) -> ToneMeasurement {
    ToneMeasurement { value, limit }
}

fn report(symptom: &str, severity: i32, suggestion: Option<ToneFix>) -> ToneReport {
    ToneReport {
        symptom: symptom.into(),
        severity,
        culprit: suggestion.as_ref().map(|s| s.block.clone()),
        culprit_label: if suggestion.is_some() {
            "Silicon Fuzz".into()
        } else {
            String::new()
        },
        fizz: measurement(0.30, 0.18),
        mud: measurement(0.10, 0.35),
        boom: measurement(0.08, 0.30),
        clip: measurement(0.00, 0.01),
        suggestion,
    }
}

fn fix(enable_path: Option<&str>) -> ToneFix {
    ToneFix {
        block: BlockId("fz".into()),
        param_path: "tone".into(),
        param_label: "Tone".into(),
        current: 70.0,
        suggested: 45.0,
        enable_path: enable_path.map(str::to_string),
        rationale: String::new(),
    }
}

#[test]
fn a_fizz_verdict_becomes_a_red_panel_with_culprit_and_move() {
    let view = view_from_report(&report("Fizz", 2, Some(fix(None))));

    assert!(view.has_result, "{view:?}");
    assert!(!view.running, "the run is over: {view:?}");
    assert_eq!(view.symptom_level, 2, "fizz is red: {view:?}");
    assert_eq!(view.symptom_text, "Fizz", "{view:?}");
    assert_eq!(view.culprit_label, "Silicon Fuzz", "{view:?}");
    assert!(view.has_suggestion, "{view:?}");
    assert_eq!(
        view.suggestion_text, "Tone 70 → 45",
        "the panel shows the measured move: {view:?}"
    );
    // The measurements travel to the panel meters.
    assert!(
        view.fizz_value > view.fizz_limit,
        "fizz over its limit: {view:?}"
    );
    assert_eq!(view.fizz_limit, 0.18, "{view:?}");
}

#[test]
fn a_gated_knob_says_which_group_gets_switched_on() {
    let view = view_from_report(&report("Mud", 1, Some(fix(Some("eq.enabled")))));

    assert_eq!(
        view.suggestion_text, "EQ on · Tone 70 → 45",
        "the panel warns that applying also switches the group on: {view:?}"
    );
}

#[test]
fn a_healthy_verdict_reads_ok_with_no_culprit() {
    let view = view_from_report(&report("Ok", 0, None));

    assert!(view.has_result, "a run happened: {view:?}");
    assert_eq!(view.symptom_level, 0, "healthy = green: {view:?}");
    assert_eq!(view.symptom_text, "OK", "on screen it reads OK: {view:?}");
    assert!(view.culprit_label.is_empty(), "{view:?}");
    assert!(!view.has_suggestion, "{view:?}");
    assert!(view.suggestion_text.is_empty(), "{view:?}");
}

#[test]
fn a_fractional_knob_keeps_one_decimal() {
    let mut f = fix(None);
    f.current = 5.5;
    f.suggested = 3.0;
    let view = view_from_report(&report("Fizz", 2, Some(f)));

    assert_eq!(view.suggestion_text, "Tone 5.5 → 3", "{view:?}");
}
