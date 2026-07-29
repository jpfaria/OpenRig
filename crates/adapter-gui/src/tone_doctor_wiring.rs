//! #791 — Tone Doctor GUI wiring: turn the dispatcher's verdict into the
//! panel's view fields.
//!
//! The diagnosis itself is NOT here: it is `Command::DiagnoseChainTone`,
//! resolved by the dispatcher so MCP and gRPC see the same verdict (the panel
//! used to run it locally, which left every other transport blind). What
//! remains is the pure, unit-testable mapping report → view.

use application::tone_doctor_report::ToneReport;

/// The panel's result fields, mirroring `ToneDoctorState` in Slint. Carries the
/// raw measurements + their healthy limits so the panel can show the meters
/// (the user sees the data behind the verdict), not just a word.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToneDoctorView {
    pub running: bool,
    pub has_result: bool,
    pub symptom_level: i32,
    pub symptom_text: String,
    pub culprit_label: String,
    pub suggestion_text: String,
    pub has_suggestion: bool,
    // Measurements (value, limit) for the meters.
    pub fizz_value: f32,
    pub fizz_limit: f32,
    pub mud_value: f32,
    pub mud_limit: f32,
    pub boom_value: f32,
    pub boom_limit: f32,
    pub clip_value: f32,
    pub clip_limit: f32,
}

/// Map a finished diagnosis onto the panel's fields.
pub fn view_from_report(report: &ToneReport) -> ToneDoctorView {
    let suggestion_text = report
        .suggestion
        .as_ref()
        .map(|s| {
            // Prefix the gate we turn on, e.g. "EQ on · Treble 7 → 4".
            let gate = s
                .enable_path
                .as_deref()
                .and_then(|p| p.strip_suffix(".enabled"))
                .map(|g| format!("{} on · ", g.to_uppercase()))
                .unwrap_or_default();
            format!(
                "{gate}{} {} → {}",
                s.param_label,
                trim_num(s.current),
                trim_num(s.suggested)
            )
        })
        .unwrap_or_default();

    ToneDoctorView {
        running: false,
        has_result: true,
        symptom_level: report.severity,
        symptom_text: symptom_text(&report.symptom).to_string(),
        culprit_label: report.culprit_label.clone(),
        has_suggestion: report.suggestion.is_some(),
        suggestion_text,
        fizz_value: report.fizz.value,
        fizz_limit: report.fizz.limit,
        mud_value: report.mud.value,
        mud_limit: report.mud.limit,
        boom_value: report.boom.value,
        boom_limit: report.boom.limit,
        clip_value: report.clip.value,
        clip_limit: report.clip.limit,
    }
}

/// Short label for a symptom. Technical terms stay as-is (they double as the
/// descriptor names); the healthy case reads "OK" on screen, not the wire name.
fn symptom_text(symptom: &str) -> &str {
    if symptom == "Ok" {
        "OK"
    } else {
        symptom
    }
}

/// Format a knob value without a trailing `.0` (e.g. `70` not `70.0`, but
/// `5.5` stays `5.5`).
fn trim_num(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}

#[cfg(test)]
#[path = "tone_doctor_wiring_tests.rs"]
mod tests;
